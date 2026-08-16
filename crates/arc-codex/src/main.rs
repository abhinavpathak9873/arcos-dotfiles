use arc_protocol::{socket_path, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{oneshot, Mutex},
};

type AppServerResult = Result<Value, String>;
type PendingRequests = Arc<Mutex<BTreeMap<u64, oneshot::Sender<AppServerResult>>>>;

struct AppServer {
    child: Child,
    input: Arc<Mutex<ChildStdin>>,
    pending: PendingRequests,
    next_id: AtomicU64,
    initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadReference {
    task_id: String,
    room_id: Option<String>,
    thread_id: String,
    cwd: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Mapping {
    #[serde(default)]
    tasks: BTreeMap<String, ThreadReference>,
    #[serde(default)]
    rooms: BTreeMap<String, String>,
}

struct State {
    server: Option<AppServer>,
    mapping: Mapping,
    mapping_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() != Some("serve") {
        println!("arc-codex {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let path = socket_path("arc-codex");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    if path.exists() {
        if UnixStream::connect(&path).await.is_ok() {
            anyhow::bail!("arc-codex is already running");
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let state_dir = std::env::var_os("ARC_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(|path| PathBuf::from(path).join("arc")))
        .or_else(|| {
            std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/state/arc"))
        })
        .unwrap_or_else(|| PathBuf::from(".arc-state"));
    std::fs::create_dir_all(&state_dir)?;
    let mapping_path = state_dir.join("codex-threads.json");
    let mapping = if mapping_path.exists() {
        serde_json::from_slice(&std::fs::read(&mapping_path)?)?
    } else {
        Mapping::default()
    };
    let state = std::sync::Arc::new(tokio::sync::Mutex::new(State {
        server: None,
        mapping,
        mapping_path,
    }));
    loop {
        let (stream, _) = listener.accept().await?;
        if stream.peer_cred()?.uid() != std::fs::metadata("/proc/self")?.uid() {
            continue;
        }
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = client(stream, state).await {
                eprintln!("arc-codex client: {error:#}");
            }
        });
    }
}

async fn client(
    stream: UnixStream,
    state: std::sync::Arc<tokio::sync::Mutex<State>>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(value) => value,
            Err(error) => {
                write(&mut writer, Response::error(0, -32700, error.to_string())).await?;
                continue;
            }
        };
        let id = request.id;
        let result = dispatch(request, &state).await;
        write(
            &mut writer,
            match result {
                Ok(value) => Response::ok(id, value),
                Err(error) => Response::error(id, -32000, error.to_string()),
            },
        )
        .await?;
    }
    Ok(())
}

async fn dispatch(request: Request, state: &tokio::sync::Mutex<State>) -> anyhow::Result<Value> {
    match request.method.as_str() {
        "health" | "codex/status" => {
            let state = state.lock().await;
            Ok(
                json!({ "status": "ready", "running": state.server.is_some(), "mappedTasks": state.mapping.tasks.len(), "mappedRooms": state.mapping.rooms.len() }),
            )
        }
        "system/stop" | "codex/interrupt" => {
            let mut state = state.lock().await;
            if let Some(mut server) = state.server.take() {
                let _ = server.child.start_kill();
            }
            Ok(json!({ "cancelled": true }))
        }
        "codex/delegate" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Params {
                task_id: String,
                room_id: Option<String>,
                cwd: String,
                prompt: String,
                model: Option<String>,
                effort: Option<String>,
            }
            let params: Params = serde_json::from_value(request.params)?;
            let mut state = state.lock().await;
            let existing_thread = state
                .mapping
                .tasks
                .get(&params.task_id)
                .map(|reference| reference.thread_id.clone())
                .or_else(|| {
                    params
                        .room_id
                        .as_ref()
                        .and_then(|room_id| state.mapping.rooms.get(room_id).cloned())
                });
            ensure_server(&mut state).await?;
            let server = state.server.as_mut().unwrap();
            server.initialize().await?;
            let thread_id = if let Some(thread_id) = existing_thread {
                server
                    .request("thread/resume", json!({ "threadId": thread_id }))
                    .await?;
                thread_id
            } else {
                let response = server.request("thread/start", json!({ "cwd": params.cwd, "model": params.model, "approvalPolicy": "never", "sandbox": "danger-full-access", "experimentalRawEvents": false })).await?;
                string_field(&response, &["thread/id", "threadId", "id"])
                    .ok_or_else(|| anyhow::anyhow!("Codex did not return a thread id"))?
            };
            let response = server.request("turn/start", json!({ "threadId": thread_id, "input": [{ "type": "text", "text": params.prompt }], "effort": params.effort })).await?;
            let reference = ThreadReference {
                task_id: params.task_id.clone(),
                room_id: params.room_id,
                thread_id: thread_id.clone(),
                cwd: params.cwd,
            };
            state
                .mapping
                .tasks
                .insert(params.task_id, reference.clone());
            if let Some(room_id) = &reference.room_id {
                state
                    .mapping
                    .rooms
                    .insert(room_id.clone(), thread_id.clone());
            }
            persist_mapping(&state.mapping_path, &state.mapping)?;
            Ok(json!({ "delegated": true, "reference": reference, "turn": response }))
        }
        method if method.starts_with("codex/") => {
            let method = method.trim_start_matches("codex/");
            let mut state = state.lock().await;
            ensure_server(&mut state).await?;
            let server = state.server.as_mut().unwrap();
            server.initialize().await?;
            server.request(method, request.params).await
        }
        _ => anyhow::bail!("method not found"),
    }
}

async fn ensure_server(state: &mut State) -> anyhow::Result<()> {
    let stopped = match state.server.as_mut() {
        Some(server) => server.child.try_wait()?.is_some(),
        None => true,
    };
    if stopped {
        state.server = Some(AppServer::start().await?);
    }
    Ok(())
}

impl AppServer {
    async fn start() -> anyhow::Result<Self> {
        let binary = std::env::var("ARC_CODEX_PATH").unwrap_or_else(|_| "codex".into());
        let mut child = Command::new(binary)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let input = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("Codex stdin unavailable"))?,
        ));
        let output = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex stdout unavailable"))?;
        let pending = Arc::new(Mutex::new(BTreeMap::new()));
        tokio::spawn(app_server_reader(output, input.clone(), pending.clone()));
        Ok(Self {
            child,
            input,
            pending,
            next_id: AtomicU64::new(0),
            initialized: false,
        })
    }

    async fn initialize(&mut self) -> anyhow::Result<()> {
        if self.initialized {
            return Ok(());
        }
        self.request("initialize", json!({ "clientInfo": { "name": "arc-codex", "title": "ArcOS Codex bridge", "version": env!("CARGO_PKG_VERSION") }, "capabilities": { "experimentalApi": true } })).await?;
        self.input
            .lock()
            .await
            .write_all(b"{\"method\":\"initialized\",\"params\":{}}\n")
            .await?;
        self.initialized = true;
        Ok(())
    }

    async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        if let Err(error) = self
            .input
            .lock()
            .await
            .write_all(
                format!(
                    "{}\n",
                    json!({ "id": id, "method": method, "params": params })
                )
                .as_bytes(),
            )
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error.into());
        }
        let response =
            match tokio::time::timeout(std::time::Duration::from_secs(120), receiver).await {
                Ok(response) => response.map_err(|_| anyhow::anyhow!("Codex app-server closed"))?,
                Err(_) => {
                    self.pending.lock().await.remove(&id);
                    anyhow::bail!("Codex {method} timed out");
                }
            };
        response.map_err(anyhow::Error::msg)
    }
}

async fn app_server_reader(
    output: ChildStdout,
    input: Arc<Mutex<ChildStdin>>,
    pending: PendingRequests,
) {
    let mut lines = BufReader::new(output).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("method").is_none() {
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                if let Some(sender) = pending.lock().await.remove(&id) {
                    let response = match value.get("error") {
                        Some(error) => Err(format!("Codex: {error}")),
                        None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = sender.send(response);
                }
            }
            continue;
        }
        if let Some(id) = value.get("id").cloned() {
            // Arc never silently approves credentials, filesystem escapes, or
            // external side effects requested by a delegated coding worker.
            report_codex_event(value.clone()).await;
            let method = value
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let response = declined_server_request(id, method);
            let _ = input
                .lock()
                .await
                .write_all(format!("{response}\n").as_bytes())
                .await;
            continue;
        }
        report_codex_event(value).await;
    }
    let mut pending = pending.lock().await;
    for (_, sender) in std::mem::take(&mut *pending) {
        let _ = sender.send(Err("Codex app-server closed".into()));
    }
}

fn declined_server_request(id: Value, method: &str) -> Value {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            json!({ "id": id, "result": { "decision": "decline" } })
        }
        "item/tool/requestUserInput" => json!({ "id": id, "result": { "answers": {} } }),
        "item/permissions/requestApproval" => {
            json!({ "id": id, "result": { "permissions": {} } })
        }
        "mcpServer/elicitation/request" => {
            json!({ "id": id, "result": { "action": "decline" } })
        }
        "applyPatchApproval" | "execCommandApproval" => {
            json!({ "id": id, "result": { "decision": "abort" } })
        }
        "currentTime/read" => {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_secs())
                .unwrap_or_default();
            json!({ "id": id, "result": { "currentTimeAt": seconds } })
        }
        _ => json!({
            "id": id,
            "error": { "code": -32601, "message": "Arc does not expose this client capability" }
        }),
    }
}

async fn report_codex_event(event: Value) {
    if let Ok(mut stream) = UnixStream::connect(socket_path("arc-core")).await {
        let request = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "codex/reportEvent".into(),
            params: json!({ "event": event }),
        };
        let _ = stream
            .write_all(
                format!("{}\n", serde_json::to_string(&request).unwrap_or_default()).as_bytes(),
            )
            .await;
    }
}

fn string_field(value: &Value, paths: &[&str]) -> Option<String> {
    for path in paths {
        let candidate = if path.contains('/') {
            value.pointer(&format!("/{path}"))
        } else {
            value.get(path)
        };
        if let Some(value) = candidate.and_then(Value::as_str) {
            return Some(value.to_owned());
        }
    }
    None
}

fn persist_mapping(path: &PathBuf, mapping: &Mapping) -> anyhow::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(mapping)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

async fn write(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: Response,
) -> anyhow::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
        .await?;
    Ok(())
}
