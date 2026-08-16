use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TurnOptions {
    pub thread_id: Option<String>,
    pub cwd: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub approval_policy: String,
    pub sandbox: String,
}

impl TurnOptions {
    pub fn maximum_autonomy(cwd: impl Into<String>) -> Self {
        Self {
            thread_id: None,
            cwd: cwd.into(),
            model: None,
            effort: None,
            approval_policy: "never".into(),
            sandbox: "danger-full-access".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Codex app-server unavailable: {0}")]
    Unavailable(String),
    #[error("Codex protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait CodexGateway: Send + Sync {
    async fn start_turn(
        &self,
        prompt: String,
        options: TurnOptions,
    ) -> Result<(String, mpsc::Receiver<Value>), GatewayError>;
    async fn steer(&self, turn_id: &str, message: String) -> Result<(), GatewayError>;
    async fn interrupt(&self, turn_id: &str) -> Result<(), GatewayError>;
    async fn list_threads(&self) -> Result<Vec<Value>, GatewayError>;
    async fn capabilities(&self) -> Result<Value, GatewayError>;
}
