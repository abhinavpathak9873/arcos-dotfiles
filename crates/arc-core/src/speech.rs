use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub confidence: f32,
    pub final_result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("speech unavailable: {0}")]
    Unavailable(String),
    #[error("speech cancelled")]
    Cancelled,
    #[error("speech operation failed: {0}")]
    Operation(String),
}

/// Native implementations own cpal capture/playback and keep model lifetime
/// outside the UI process. Cancellation must complete promptly and release the
/// microphone before returning.
#[async_trait]
pub trait SpeechEngine: Send + Sync {
    async fn start_capture(&self, hotwords: &[String]) -> Result<(), SpeechError>;
    async fn finish_capture(&self) -> Result<Transcript, SpeechError>;
    async fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<(), SpeechError>;
    async fn voices(&self) -> Result<Vec<Voice>, SpeechError>;
    async fn cancel(&self) -> Result<(), SpeechError>;
    async fn unload_models(&self) -> Result<(), SpeechError>;
}
