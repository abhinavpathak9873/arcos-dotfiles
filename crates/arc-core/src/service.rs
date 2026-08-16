use crate::{
    activity::ActivityStore,
    apps::AppRegistry,
    audit::{AuditStore, ReceiptInput},
    desktop::SwayAdapter,
    model_router,
    policy::{evaluate, ActionClass},
    rooms::{EvidenceInput, RoomStore},
    utterance::{UtteranceRecord, UtteranceStore},
};
use arc_protocol::{
    ActionOutcome, EventKind, EvidenceKind, PermissionDecision, Request, Response, RoomResource,
    PROTOCOL_VERSION,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use uuid::Uuid;

pub struct CoreService {
    state_dir: PathBuf,
    apps: AppRegistry,
    rooms: RoomStore,
    audit: AuditStore,
    utterances: UtteranceStore,
    activity: ActivityStore,
    desktop: SwayAdapter,
    pending_events: Vec<PendingEvent>,
}

#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub kind: EventKind,
    pub payload: Value,
}

impl CoreService {
    pub fn open_default() -> anyhow::Result<Self> {
        let state_dir = env::var_os("ARC_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| env::var_os("XDG_STATE_HOME").map(|path| PathBuf::from(path).join("arc")))
            .or_else(|| {
                env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/state/arc"))
            })
            .unwrap_or_else(|| PathBuf::from(".arc-state"));
        Self::open(state_dir)
    }

    pub fn open(state_dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&state_dir)?;
        Ok(Self {
            apps: AppRegistry::open_default(&state_dir)?,
            rooms: RoomStore::open(state_dir.join("rooms.json"))?,
            audit: AuditStore::open(state_dir.join("audit.jsonl"))?,
            utterances: UtteranceStore::open(state_dir.join("utterances.json"))?,
            activity: ActivityStore::open(state_dir.join("activity.json"))?,
            desktop: SwayAdapter::default(),
            pending_events: Vec::new(),
            state_dir,
        })
    }

    pub fn drain_events(&mut self) -> Vec<PendingEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn handle(&mut self, request: Request) -> Response {
        if request.jsonrpc != "2.0" {
            return Response::error(request.id, -32600, "jsonrpc must be 2.0");
        }
        match self.dispatch(&request.method, request.params) {
            Ok(value) => Response::ok(request.id, value),
            Err(CoreRpcError::Invalid(message)) => Response::error(request.id, -32602, message),
            Err(CoreRpcError::NotFound(message)) => Response::error(request.id, -32004, message),
            Err(CoreRpcError::Failed(message)) => Response::error(request.id, -32000, message),
        }
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, CoreRpcError> {
        match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "coreVersion": env!("CARGO_PKG_VERSION"),
                "identity": "arc",
                "kernel": "hermes",
                "transport": "unix_socket",
                "socketAuthentication": "unix_peer_credentials",
                "capabilities": {
                    "conversation": ["route", "submit", "interrupt", "activity"],
                    "shell": ["prompt", "toggle", "collapse"],
                    "confirmations": ["respond"],
                    "speech": ["start", "finish", "toggle", "cancel", "speak", "sanitize", "configure"],
                    "tasks": ["list", "cancel"],
                    "apps": ["query", "get", "refresh", "launch", "learn_alias"],
                    "windows": ["list", "focus", "move"],
                    "workspaces": ["list", "focus"],
                    "input": ["pointer_move", "click", "clipboard_read", "clipboard_write", "stop"],
                    "context": ["snapshot", "capture_screen"],
                    "projectRooms": ["create", "list", "get", "archive", "resources", "evidence", "timeline", "layout_snapshot"],
                    "policy": ["evaluate"],
                    "audit": ["append", "list", "verify"],
                    "events": ["subscribe"],
                    "desktopInput": true,
                    "screenCapture": true,
                    "privilegedSystem": false
                }
            })),
            "health" => Ok(json!({
                "status": "ready",
                "protocolVersion": PROTOCOL_VERSION,
                "stateDirectory": self.state_dir,
                "auditValid": self.audit.verify().is_ok()
            })),
            "turn/cancel" | "system/stop" => {
                let cancelled_tasks = self.utterances.cancel_active().map_err(failed)?;
                let receipt = self.append_receipt(ReceiptInput {
                    actor: "user".into(),
                    action: "system.stop".into(),
                    target: "active_agent_work".into(),
                    outcome: ActionOutcome::Cancelled,
                    reversible: true,
                    permission: PermissionDecision::Allowed,
                    detail: "Hard stop requested below the agent layer".into(),
                })?;
                self.emit(
                    EventKind::HardStop,
                    json!({ "cancelledTasks": cancelled_tasks, "deadlineMs": 100 }),
                );
                Ok(
                    json!({ "cancelled": true, "cancelledTasks": cancelled_tasks, "receipt": receipt }),
                )
            }
            "events/subscribe" => Ok(json!({ "subscribed": true })),
            "activity/list" | "conversation/activity" => {
                #[derive(Deserialize, Default)]
                struct Params {
                    limit: Option<usize>,
                }
                let value: Params = from_params(params)?;
                Ok(json!({ "items": self.activity.items(value.limit.unwrap_or(30)) }))
            }
            "conversation/route" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    text: String,
                    requested_model: Option<String>,
                    requested_effort: Option<String>,
                }
                let value: Params = from_params(params)?;
                let route = model_router::route(
                    &value.text,
                    value.requested_model.as_deref(),
                    value.requested_effort.as_deref(),
                );
                Ok(json!({ "route": route }))
            }
            "conversation/submit" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    utterance_id: Uuid,
                    text: String,
                    room_id: Option<Uuid>,
                    hermes_session_id: Option<String>,
                    requested_model: Option<String>,
                    requested_effort: Option<String>,
                }
                let value: Params = from_params(params)?;
                if value.text.trim().is_empty() {
                    return Err(CoreRpcError::Invalid("text cannot be empty".into()));
                }
                let route = model_router::route(
                    &value.text,
                    value.requested_model.as_deref(),
                    value.requested_effort.as_deref(),
                );
                let record = UtteranceRecord::accepted(
                    value.utterance_id,
                    value.text.trim().into(),
                    route.clone(),
                    value.room_id,
                    value.hermes_session_id,
                );
                let (record, duplicate) = self.utterances.accept(record).map_err(failed)?;
                if !duplicate {
                    self.activity
                        .user_message(record.id, &record.text)
                        .map_err(failed)?;
                    self.emit(
                        EventKind::ModelRouting,
                        json!({ "utteranceId": record.id, "route": route }),
                    );
                    self.emit(EventKind::TaskProgress, json!({ "utteranceId": record.id, "state": "accepted", "delegateToCodex": record.route.delegate_to_codex }));
                }
                Ok(json!({ "accepted": true, "duplicate": duplicate, "utterance": record }))
            }
            "conversation/interrupt" | "tasks/cancel" => {
                let count = self.utterances.cancel_active().map_err(failed)?;
                self.emit(
                    EventKind::TaskProgress,
                    json!({ "state": "cancelled", "count": count }),
                );
                Ok(json!({ "cancelled": count }))
            }
            "tasks/list" => Ok(json!({ "tasks": self.utterances.list() })),
            "speech/start" => {
                self.emit(EventKind::SpeechState, json!({ "state": "listening" }));
                Ok(json!({ "requested": true, "service": "arc-speech" }))
            }
            "speech/finish" => {
                self.emit(EventKind::SpeechState, json!({ "state": "transcribing" }));
                Ok(json!({ "requested": true, "service": "arc-speech" }))
            }
            "speech/toggle" => {
                self.emit(
                    EventKind::SpeechState,
                    json!({ "state": "toggle_requested" }),
                );
                Ok(json!({ "requested": true, "service": "arc-speech" }))
            }
            "speech/cancel" => {
                self.emit(
                    EventKind::SpeechState,
                    json!({ "state": "idle", "cancelled": true }),
                );
                Ok(json!({ "cancelled": true }))
            }
            "speech/reportTranscript" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    utterance_id: Uuid,
                    text: String,
                    #[serde(default)]
                    stable: bool,
                }
                let value: Params = from_params(params)?;
                self.emit(
                    if value.stable {
                        EventKind::TranscriptStable
                    } else {
                        EventKind::TranscriptProvisional
                    },
                    json!({ "utteranceId": value.utterance_id, "text": value.text }),
                );
                Ok(json!({ "reported": true }))
            }
            "speech/reportState" => {
                #[derive(Deserialize)]
                struct Params {
                    state: String,
                    text: Option<String>,
                }
                let value: Params = from_params(params)?;
                self.emit(
                    EventKind::SpeechState,
                    json!({ "state": value.state, "text": value.text }),
                );
                Ok(json!({ "reported": true }))
            }
            "speech/speak" | "speech/sanitize" | "speech/configure" => {
                Ok(json!({ "requested": true, "service": "arc-speech" }))
            }
            "shell/prompt" => {
                self.emit(
                    EventKind::DesktopControlState,
                    json!({ "textPrompt": "open" }),
                );
                Ok(json!({ "opened": true }))
            }
            "shell/toggle" => {
                self.emit(EventKind::DesktopControlState, json!({ "shell": "toggle" }));
                Ok(json!({ "toggled": true }))
            }
            "shell/collapse" => {
                self.emit(
                    EventKind::DesktopControlState,
                    json!({ "shell": "collapse" }),
                );
                Ok(json!({ "collapsed": true }))
            }
            "confirmations/respond" => {
                #[derive(Deserialize)]
                struct Params {
                    id: String,
                    allow: bool,
                }
                let value: Params = from_params(params)?;
                self.emit(
                    EventKind::TaskProgress,
                    json!({
                        "confirmationId": value.id,
                        "state": if value.allow { "confirmed" } else { "denied" }
                    }),
                );
                Ok(json!({ "recorded": true, "allowed": value.allow }))
            }
            "codex/reportEvent" => {
                let event = params.get("event").cloned().unwrap_or(Value::Null);
                let method = event
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let kind = if method.contains("requestApproval")
                    || method.contains("requestUserInput")
                    || method.contains("elicitation")
                {
                    EventKind::ConfirmationRequested
                } else if method.contains("command")
                    || method.contains("file")
                    || method.contains("tool")
                {
                    EventKind::ToolCall
                } else {
                    EventKind::TaskProgress
                };
                self.emit(kind, json!({ "codex": event }));
                Ok(json!({ "reported": true }))
            }
            method if method.starts_with("codex/") => {
                Ok(json!({ "requested": true, "service": "arc-codex" }))
            }
            "apps/query" | "apps/list" => {
                #[derive(Deserialize, Default)]
                struct Params {
                    #[serde(default)]
                    query: String,
                    limit: Option<usize>,
                }
                let value: Params = from_params(params)?;
                Ok(json!({ "apps": self.apps.query(&value.query, value.limit.unwrap_or(100)) }))
            }
            "apps/get" => {
                #[derive(Deserialize)]
                struct Params {
                    id: String,
                }
                let value: Params = from_params(params)?;
                let app = self
                    .apps
                    .get(&value.id)
                    .map_err(|error| CoreRpcError::NotFound(error.to_string()))?;
                Ok(json!({ "app": app }))
            }
            "apps/refresh" => {
                let count = self.apps.refresh().map_err(failed)?;
                Ok(json!({ "count": count }))
            }
            "apps/learnAlias" | "apps/learn_alias" => {
                #[derive(Deserialize)]
                struct Params {
                    id: String,
                    alias: String,
                }
                let value: Params = from_params(params)?;
                self.apps
                    .add_alias(&value.id, &value.alias)
                    .map_err(failed)?;
                let receipt = self.append_receipt(ReceiptInput {
                    actor: "user".into(),
                    action: "apps.learn_alias".into(),
                    target: value.id,
                    outcome: ActionOutcome::Succeeded,
                    reversible: true,
                    permission: PermissionDecision::Allowed,
                    detail: format!("Learned application alias {}", value.alias),
                })?;
                Ok(json!({ "updated": true, "receipt": receipt }))
            }
            "apps/launch" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    id: String,
                    #[serde(default)]
                    room_id: Option<Uuid>,
                }
                let value: Params = from_params(params)?;
                let app = self
                    .apps
                    .get(&value.id)
                    .map_err(|error| CoreRpcError::NotFound(error.to_string()))?
                    .clone();
                let policy = evaluate(ActionClass::Normal, false);
                let launch = self.apps.launch(&value.id);
                let (outcome, detail) = match &launch {
                    Ok(()) => (ActionOutcome::Succeeded, format!("Launched {}", app.name)),
                    Err(error) => (ActionOutcome::Failed, error.to_string()),
                };
                let receipt = self.append_receipt(ReceiptInput {
                    actor: "arc".into(),
                    action: "apps.launch".into(),
                    target: value.id.clone(),
                    outcome,
                    reversible: true,
                    permission: policy.decision,
                    detail: detail.clone(),
                })?;
                if let Some(room_id) = value.room_id {
                    self.rooms
                        .record_event(
                            room_id,
                            "arc".into(),
                            "apps.launch".into(),
                            detail.clone(),
                            Some(receipt.id),
                            json!({ "appId": value.id }),
                        )
                        .map_err(failed)?;
                }
                Ok(json!({
                    "launched": launch.is_ok(),
                    "app": app,
                    "error": launch.err().map(|error| error.to_string()),
                    "receipt": receipt
                }))
            }
            "apps/focus" => Err(CoreRpcError::Invalid(
                "apps/focus requires a window id; use windows/focus".into(),
            )),
            "windows/list" => Ok(json!({ "windows": self.desktop.windows().map_err(failed)? })),
            "windows/focus" => {
                #[derive(Deserialize)]
                struct Params {
                    id: i64,
                }
                let value: Params = from_params(params)?;
                let verified = self.desktop.focus_window(value.id).map_err(failed)?;
                let receipt = self.desktop_receipt(
                    "windows.focus",
                    value.id.to_string(),
                    verified,
                    "Sway focus followed by tree verification",
                )?;
                Ok(json!({ "focused": verified, "verified": verified, "receipt": receipt }))
            }
            "windows/move" => {
                #[derive(Deserialize)]
                struct Params {
                    id: i64,
                    workspace: String,
                }
                let value: Params = from_params(params)?;
                let verified = self
                    .desktop
                    .move_window(value.id, &value.workspace)
                    .map_err(failed)?;
                let receipt = self.desktop_receipt(
                    "windows.move",
                    value.id.to_string(),
                    verified,
                    &format!("Verified window on workspace {}", value.workspace),
                )?;
                Ok(json!({ "moved": verified, "verified": verified, "receipt": receipt }))
            }
            "workspaces/list" => {
                Ok(json!({ "workspaces": self.desktop.workspaces().map_err(failed)? }))
            }
            "workspaces/focus" => {
                #[derive(Deserialize)]
                struct Params {
                    workspace: String,
                }
                let value: Params = from_params(params)?;
                let verified = self
                    .desktop
                    .focus_workspace(&value.workspace)
                    .map_err(failed)?;
                let receipt = self.desktop_receipt(
                    "workspaces.focus",
                    value.workspace,
                    verified,
                    "Sway workspace focus verified",
                )?;
                Ok(json!({ "focused": verified, "verified": verified, "receipt": receipt }))
            }
            "system/outputs" => Ok(json!({ "outputs": self.desktop.outputs().map_err(failed)? })),
            "input/stop" => {
                self.emit(EventKind::DesktopControlState, json!({ "active": false }));
                Ok(json!({ "stopped": true }))
            }
            "input/pointerMove" => {
                #[derive(Deserialize)]
                struct Params {
                    x: f64,
                    y: f64,
                }
                let value: Params = from_params(params)?;
                let verified = self
                    .desktop
                    .move_agent_pointer(crate::desktop::Point {
                        x: value.x,
                        y: value.y,
                    })
                    .map_err(failed)?;
                let receipt = self.desktop_receipt(
                    "input.pointer_move",
                    "agent-seat".into(),
                    verified,
                    "Sway routed pointer movement to the hidden Arc agent seat",
                )?;
                Ok(
                    json!({ "moved": true, "verified": verified, "seat": "agent-seat", "receipt": receipt }),
                )
            }
            "input/click" => {
                #[derive(Deserialize)]
                struct Params {
                    #[serde(default = "default_pointer_button")]
                    button: u8,
                }
                let value: Params = from_params(params)?;
                let verified = self
                    .desktop
                    .click_agent_pointer(value.button)
                    .map_err(failed)?;
                let receipt = self.desktop_receipt(
                    "input.click",
                    "agent-seat".into(),
                    verified,
                    &format!(
                        "Sway delivered button{} on the Arc agent seat",
                        value.button
                    ),
                )?;
                Ok(
                    json!({ "clicked": true, "verified": verified, "seat": "agent-seat", "receipt": receipt }),
                )
            }
            "input/clipboardRead" => {
                let output = Command::new("wl-paste")
                    .arg("--no-newline")
                    .output()
                    .map_err(failed)?;
                if !output.status.success() {
                    return Err(CoreRpcError::Failed(
                        String::from_utf8_lossy(&output.stderr).into(),
                    ));
                }
                let value = String::from_utf8(output.stdout).map_err(failed)?;
                self.desktop_receipt(
                    "clipboard.read",
                    "system_clipboard".into(),
                    true,
                    &format!("Read {} characters", value.chars().count()),
                )?;
                Ok(json!(value))
            }
            "input/clipboardWrite" => {
                #[derive(Deserialize)]
                struct Params {
                    value: String,
                }
                let value: Params = from_params(params)?;
                let mut child = Command::new("wl-copy")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .spawn()
                    .map_err(failed)?;
                child
                    .stdin
                    .take()
                    .ok_or_else(|| CoreRpcError::Failed("wl-copy stdin unavailable".into()))?
                    .write_all(value.value.as_bytes())
                    .map_err(failed)?;
                let status = child.wait().map_err(failed)?;
                if !status.success() {
                    return Err(CoreRpcError::Failed("wl-copy failed".into()));
                }
                let receipt = self.desktop_receipt(
                    "clipboard.write",
                    "system_clipboard".into(),
                    true,
                    &format!("Wrote {} characters", value.value.chars().count()),
                )?;
                Ok(json!({ "written": true, "receipt": receipt }))
            }
            "context/captureScreen" => {
                let path = env::temp_dir().join(format!("arc-capture-{}.png", Uuid::new_v4()));
                let output = Command::new("grim").arg(&path).output().map_err(failed)?;
                if !output.status.success() {
                    return Err(CoreRpcError::Failed(
                        String::from_utf8_lossy(&output.stderr).into(),
                    ));
                }
                let bytes = fs::read(&path).map_err(failed)?;
                let _ = fs::remove_file(&path);
                let _receipt = self.desktop_receipt(
                    "screen.capture",
                    "active_display".into(),
                    true,
                    "Captured and verified one ephemeral frame",
                )?;
                Ok(json!(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )))
            }
            "context/snapshot" => {
                #[derive(Deserialize, Default)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    #[serde(default)]
                    include_sensitive: bool,
                    room_id: Option<Uuid>,
                }
                let value: Params = from_params(params)?;
                let focused = self
                    .desktop
                    .focused(value.include_sensitive)
                    .unwrap_or_default();
                Ok(json!({
                    "capturedAt": chrono::Utc::now().to_rfc3339(),
                    "desktop": env::var("XDG_CURRENT_DESKTOP").ok(),
                    "sessionType": env::var("XDG_SESSION_TYPE").ok(),
                    "waylandDisplay": env::var("WAYLAND_DISPLAY").ok(),
                    "focused": focused,
                    "currentProjectRoom": value.room_id,
                    "screenCaptureActive": false,
                    "microphoneActive": false,
                    "desktopInputActive": false
                }))
            }
            "system/hermesIdentity" => {
                let paths = hermes_identity_paths();
                Ok(json!({
                    "soul": fs::read_to_string(&paths[0]).unwrap_or_default(),
                    "memory": fs::read_to_string(&paths[1]).unwrap_or_default(),
                    "user": fs::read_to_string(&paths[2]).unwrap_or_default()
                }))
            }
            "system/saveHermesIdentity" => {
                #[derive(Deserialize)]
                struct Params {
                    key: String,
                    content: String,
                }
                let value: Params = from_params(params)?;
                let index = match value.key.as_str() {
                    "soul" => 0,
                    "memory" => 1,
                    "user" => 2,
                    _ => {
                        return Err(CoreRpcError::Invalid(
                            "identity key is not allow-listed".into(),
                        ))
                    }
                };
                let paths = hermes_identity_paths();
                if let Some(parent) = paths[index].parent() {
                    fs::create_dir_all(parent).map_err(failed)?;
                }
                let temporary = paths[index].with_extension("tmp");
                fs::write(&temporary, value.content).map_err(failed)?;
                fs::rename(temporary, &paths[index]).map_err(failed)?;
                Ok(json!({ "ok": true }))
            }
            "rooms/list" => {
                #[derive(Deserialize, Default)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    #[serde(default)]
                    include_archived: bool,
                }
                let value: Params = from_params(params)?;
                Ok(json!({ "rooms": self.rooms.list(value.include_archived) }))
            }
            "rooms/create" => {
                #[derive(Deserialize)]
                struct Params {
                    name: String,
                }
                let value: Params = from_params(params)?;
                let room = self.rooms.create(value.name).map_err(failed)?;
                Ok(json!({ "room": room }))
            }
            "rooms/get" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                }
                let value: Params = from_params(params)?;
                let room = self.rooms.get(value.room_id).map_err(not_found)?;
                Ok(json!({ "room": room }))
            }
            "rooms/archive" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                    #[serde(default = "default_true")]
                    archived: bool,
                }
                let value: Params = from_params(params)?;
                self.rooms
                    .archive(value.room_id, value.archived)
                    .map_err(not_found)?;
                Ok(json!({ "archived": value.archived }))
            }
            "rooms/addResource" | "rooms/add_resource" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                    resource: RoomResource,
                }
                let value: Params = from_params(params)?;
                let room = self
                    .rooms
                    .add_resource(value.room_id, value.resource)
                    .map_err(failed)?;
                Ok(json!({ "room": room }))
            }
            "evidence/add" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                    kind: EvidenceKind,
                    title: String,
                    uri: String,
                    source_uri: Option<String>,
                    claim: Option<String>,
                    #[serde(default)]
                    metadata: Value,
                }
                let value: Params = from_params(params)?;
                let evidence = self
                    .rooms
                    .add_evidence(
                        value.room_id,
                        EvidenceInput {
                            kind: value.kind,
                            title: value.title,
                            uri: value.uri,
                            source_uri: value.source_uri,
                            claim: value.claim,
                            metadata: value.metadata,
                        },
                    )
                    .map_err(failed)?;
                Ok(json!({ "evidence": evidence }))
            }
            "rooms/recordEvent" | "rooms/record_event" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                    actor: String,
                    action: String,
                    summary: String,
                    receipt_id: Option<Uuid>,
                    #[serde(default)]
                    metadata: Value,
                }
                let value: Params = from_params(params)?;
                let event = self
                    .rooms
                    .record_event(
                        value.room_id,
                        value.actor,
                        value.action,
                        value.summary,
                        value.receipt_id,
                        value.metadata,
                    )
                    .map_err(failed)?;
                Ok(json!({ "event": event }))
            }
            "rooms/snapshot" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    room_id: Uuid,
                    layout: Value,
                }
                let value: Params = from_params(params)?;
                self.rooms
                    .snapshot_layout(value.room_id, value.layout)
                    .map_err(failed)?;
                Ok(json!({ "saved": true }))
            }
            "policy/evaluate" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    class: ActionClass,
                    #[serde(default)]
                    explicitly_confirmed: bool,
                }
                let value: Params = from_params(params)?;
                let evaluation = evaluate(value.class, value.explicitly_confirmed);
                if evaluation.decision == PermissionDecision::ConfirmationRequired {
                    self.emit(
                        EventKind::ConfirmationRequested,
                        json!({ "class": value.class, "summary": evaluation.reason }),
                    );
                }
                Ok(serde_json::to_value(evaluation)?)
            }
            "audit/list" => {
                #[derive(Deserialize, Default)]
                struct Params {
                    limit: Option<usize>,
                }
                let value: Params = from_params(params)?;
                let limit = value.limit.unwrap_or(100).clamp(1, 1000);
                let start = self.audit.receipts().len().saturating_sub(limit);
                Ok(json!({ "receipts": &self.audit.receipts()[start..] }))
            }
            "audit/verify" => {
                self.audit.verify().map_err(failed)?;
                Ok(json!({ "valid": true, "count": self.audit.receipts().len() }))
            }
            "actions/record" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Params {
                    actor: String,
                    action: String,
                    target: String,
                    outcome: ActionOutcome,
                    reversible: bool,
                    permission: PermissionDecision,
                    detail: String,
                }
                let value: Params = from_params(params)?;
                let receipt = self.append_receipt(ReceiptInput {
                    actor: value.actor,
                    action: value.action,
                    target: value.target,
                    outcome: value.outcome,
                    reversible: value.reversible,
                    permission: value.permission,
                    detail: value.detail,
                })?;
                Ok(json!({ "receipt": receipt }))
            }
            _ => Err(CoreRpcError::NotFound("method not found".into())),
        }
    }

    fn append_receipt(
        &mut self,
        input: ReceiptInput,
    ) -> Result<arc_protocol::ActionReceipt, CoreRpcError> {
        let receipt = self.audit.append(input).map_err(failed)?;
        self.activity.receipt(&receipt).map_err(failed)?;
        Ok(receipt)
    }

    fn emit(&mut self, kind: EventKind, payload: Value) {
        let _ = self.activity.record_event(&kind, &payload);
        self.pending_events.push(PendingEvent { kind, payload });
    }

    pub fn record_external_event(&mut self, kind: &EventKind, payload: &Value) {
        let _ = self.activity.record_event(kind, payload);
    }

    fn desktop_receipt(
        &mut self,
        action: &str,
        target: String,
        verified: bool,
        detail: &str,
    ) -> Result<arc_protocol::ActionReceipt, CoreRpcError> {
        self.append_receipt(ReceiptInput {
            actor: "arc".into(),
            action: action.into(),
            target,
            outcome: if verified {
                ActionOutcome::Succeeded
            } else {
                ActionOutcome::Failed
            },
            reversible: true,
            permission: PermissionDecision::Allowed,
            detail: detail.into(),
        })
    }
}

#[derive(Debug)]
enum CoreRpcError {
    Invalid(String),
    NotFound(String),
    Failed(String),
}

impl From<serde_json::Error> for CoreRpcError {
    fn from(error: serde_json::Error) -> Self {
        CoreRpcError::Failed(error.to_string())
    }
}

fn from_params<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, CoreRpcError> {
    serde_json::from_value(if params.is_null() { json!({}) } else { params })
        .map_err(|error| CoreRpcError::Invalid(error.to_string()))
}

fn failed(error: impl std::fmt::Display) -> CoreRpcError {
    CoreRpcError::Failed(error.to_string())
}

fn not_found(error: impl std::fmt::Display) -> CoreRpcError {
    CoreRpcError::NotFound(error.to_string())
}

fn default_true() -> bool {
    true
}

fn default_pointer_button() -> u8 {
    1
}

fn hermes_identity_paths() -> [PathBuf; 3] {
    let home = env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".hermes")))
        .unwrap_or_else(|| PathBuf::from(".hermes"));
    [
        home.join("SOUL.md"),
        home.join("memories/MEMORY.md"),
        home.join("memories/USER.md"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_protocol::Request;

    fn request(id: u64, method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn project_room_rpc_round_trip_and_receipts_work() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = CoreService::open(directory.path().into()).unwrap();
        let created = service.handle(request(1, "rooms/create", json!({ "name": "ArcOS" })));
        let room_id = created.result.unwrap()["room"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let evidence = service.handle(request(
            2,
            "evidence/add",
            json!({
                "roomId": room_id,
                "kind": "webpage",
                "title": "Source",
                "uri": "https://example.test",
                "claim": "A cited claim"
            }),
        ));
        assert!(evidence.error.is_none());
        let rooms = service.handle(request(3, "rooms/list", json!({})));
        assert_eq!(
            rooms.result.unwrap()["rooms"][0]["evidence"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let recorded = service.handle(request(
            4,
            "actions/record",
            json!({
                "actor": "arc",
                "action": "screen.capture",
                "target": "display:0",
                "outcome": "succeeded",
                "reversible": true,
                "permission": "allowed",
                "detail": "One-shot capture"
            }),
        ));
        assert_eq!(recorded.result.unwrap()["receipt"]["sequence"], 1);
        let verified = service.handle(request(5, "audit/verify", json!({})));
        assert_eq!(verified.result.unwrap()["valid"], true);
    }

    #[test]
    fn invalid_params_have_json_rpc_error_code() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = CoreService::open(directory.path().into()).unwrap();
        let response = service.handle(request(1, "rooms/create", json!({})));
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[test]
    fn malformed_protocol_and_unknown_methods_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = CoreService::open(directory.path().into()).unwrap();
        let mut bad_version = request(1, "health", json!({}));
        bad_version.jsonrpc = "1.0".into();
        assert_eq!(service.handle(bad_version).error.unwrap().code, -32600);
        assert_eq!(
            service
                .handle(request(2, "unknown/method", json!({})))
                .error
                .unwrap()
                .code,
            -32004
        );
    }

    #[test]
    fn utterance_submission_emits_route_once_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        {
            let mut service = CoreService::open(directory.path().into()).unwrap();
            let first = service.handle(request(
                1,
                "conversation/submit",
                json!({ "utteranceId": id, "text": "Implement the fix in this repository" }),
            ));
            assert_eq!(first.result.as_ref().unwrap()["duplicate"], false);
            assert_eq!(
                first.result.as_ref().unwrap()["utterance"]["route"]["model"],
                "gpt-5.6-terra"
            );
            assert_eq!(service.drain_events().len(), 2);
        }
        let mut restored = CoreService::open(directory.path().into()).unwrap();
        let second = restored.handle(request(
            2,
            "conversation/submit",
            json!({ "utteranceId": id, "text": "Implement the fix in this repository" }),
        ));
        assert_eq!(second.result.unwrap()["duplicate"], true);
        assert!(restored.drain_events().is_empty());
    }
}
