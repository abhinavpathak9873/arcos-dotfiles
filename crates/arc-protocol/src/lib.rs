use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 3;

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("ARC_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR").map(|path| PathBuf::from(path).join("arc")))
        .unwrap_or_else(|| {
            let uid = std::env::var("UID").unwrap_or_else(|_| "unknown".into());
            PathBuf::from("/tmp").join(format!("arc-{uid}"))
        })
}

pub fn socket_path(service: &str) -> PathBuf {
    runtime_dir().join(format!("{service}.sock"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// A server-pushed JSON-RPC notification. Clients subscribe with
/// `events/subscribe`; notifications never consume a request id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Event,
}

impl Notification {
    pub fn event(event: Event) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: "event".into(),
            params: event,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub sequence: u64,
    pub at: String,
    pub kind: EventKind,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    TranscriptProvisional,
    TranscriptStable,
    AssistantTextDelta,
    AssistantTextComplete,
    SpeechState,
    TaskProgress,
    ToolCall,
    ModelRouting,
    ConfirmationRequested,
    DesktopControlState,
    ServiceHealth,
    HardStop,
}

/// A durable, UI-independent projection of what Arc has said and done. The
/// layer shell and the optional inspector both render this data; neither owns
/// conversation state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: Uuid,
    pub at: String,
    pub kind: ActivityKind,
    pub title: String,
    pub body: String,
    pub state: ActivityState,
    pub task_id: Option<String>,
    pub receipt_id: Option<Uuid>,
    pub source_uri: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    UserMessage,
    AssistantMessage,
    Transcript,
    Task,
    Tool,
    Receipt,
    Confirmation,
    System,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Provisional,
    Active,
    Complete,
    NeedsAttention,
    Cancelled,
    Failed,
}

impl EventKind {
    pub fn topic(&self) -> &'static str {
        match self {
            Self::TranscriptProvisional | Self::TranscriptStable => "transcripts",
            Self::AssistantTextDelta | Self::AssistantTextComplete => "conversation",
            Self::SpeechState => "speech",
            Self::TaskProgress => "tasks",
            Self::ToolCall => "tools",
            Self::ModelRouting => "models",
            Self::ConfirmationRequested => "confirmations",
            Self::DesktopControlState => "desktop",
            Self::ServiceHealth => "health",
            Self::HardStop => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    Instant,
    Fast,
    Deep,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRoute {
    pub model: String,
    pub effort: String,
    pub tier: ModelTier,
    pub reason: String,
    pub delegate_to_codex: bool,
}

impl Response {
    pub fn ok(id: u64, value: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(value).expect("serializable response")),
            error: None,
        }
    }
    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    DesktopEntry,
    Flatpak,
    AppImage,
    Path,
    Steam,
    Learned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ControlRoute {
    NativeApi,
    Dbus,
    Mpris,
    BrowserCdp,
    Accessibility,
    VirtualInput,
    Visual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppDescriptor {
    pub id: String,
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub control_routes: Vec<ControlRoute>,
    pub source: AppSource,
    pub version: Option<String>,
    pub last_used_at: Option<String>,
    pub launch_success_rate: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Webpage,
    Image,
    Video,
    Document,
    Map,
    Chart,
    ApplicationState,
    Result,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceItem {
    pub id: Uuid,
    pub kind: EvidenceKind,
    pub title: String,
    pub uri: String,
    pub source_uri: Option<String>,
    pub claim: Option<String>,
    pub captured_at: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomResource {
    pub kind: String,
    pub uri: String,
    pub title: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub summary: String,
    pub receipt_id: Option<Uuid>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRoom {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived: bool,
    pub sway_layout: Option<Value>,
    #[serde(default)]
    pub resources: Vec<RoomResource>,
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowDescriptor {
    pub id: i64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub process_id: Option<u32>,
    pub workspace: Option<String>,
    pub focused: bool,
    pub rect: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    #[serde(rename = "num")]
    pub number: i64,
    pub name: String,
    pub output: String,
    pub focused: bool,
    pub visible: bool,
    pub rect: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allowed,
    ConfirmationRequired,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionReceipt {
    pub id: Uuid,
    pub sequence: u64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub outcome: ActionOutcome,
    pub reversible: bool,
    pub permission: PermissionDecision,
    pub detail: String,
    pub previous_hash: Option<String>,
    pub hash: String,
}
