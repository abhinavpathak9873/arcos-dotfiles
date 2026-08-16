use arc_protocol::{ActivityItem, Event, EventKind};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceMode {
    Hidden,
    Capsule,
    Expanded,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening,
    Transcribing,
    Thinking,
    Acting,
    Speaking,
    NeedsAttention,
    Stopped,
    Error,
}

pub struct ShellState {
    pub surface: SurfaceMode,
    pub phase: Phase,
    pub headline: String,
    pub detail: String,
    pub prompt: String,
    pub activity: Vec<ActivityItem>,
    pub confirmation: Option<String>,
    hide_after: Option<Instant>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            surface: SurfaceMode::Hidden,
            phase: Phase::Idle,
            headline: "Arc".into(),
            detail: String::new(),
            prompt: String::new(),
            activity: Vec::new(),
            confirmation: None,
            hide_after: None,
        }
    }
}

impl ShellState {
    pub fn handle(&mut self, event: &Event) {
        match event.kind {
            EventKind::TranscriptProvisional | EventKind::TranscriptStable => {
                self.phase = Phase::Listening;
                self.headline = "Listening".into();
                self.detail = text(&event.payload).unwrap_or_else(|| "Speak naturally".into());
                self.show_capsule();
            }
            EventKind::AssistantTextDelta => {
                self.phase = Phase::Thinking;
                self.headline = "Arc".into();
                if let Some(value) = text(&event.payload) {
                    self.detail.push_str(&value);
                    truncate(&mut self.detail, 180);
                }
                self.show_capsule();
            }
            EventKind::AssistantTextComplete => {
                self.phase = Phase::Speaking;
                self.headline = "Arc".into();
                if let Some(value) = text(&event.payload) {
                    self.detail = value;
                    truncate(&mut self.detail, 180);
                }
                self.show_capsule();
                self.hide_after = Some(Instant::now() + Duration::from_secs(5));
            }
            EventKind::TaskProgress | EventKind::ModelRouting => {
                self.phase = Phase::Thinking;
                self.headline = if matches!(event.kind, EventKind::ModelRouting) {
                    "Thinking"
                } else {
                    "Working"
                }
                .into();
                self.detail = task_detail(event);
                self.show_capsule();
            }
            EventKind::ToolCall => {
                self.phase = Phase::Acting;
                self.headline = "Using the desktop".into();
                self.detail = text(&event.payload).unwrap_or_else(|| "Action in progress".into());
                self.show_capsule();
            }
            EventKind::ConfirmationRequested => {
                self.phase = Phase::NeedsAttention;
                self.headline = "Your confirmation is needed".into();
                self.detail = event
                    .payload
                    .get("summary")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Review this action before Arc continues")
                    .into();
                self.confirmation = event
                    .payload
                    .get("confirmationId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| Some("current".into()));
                self.surface = SurfaceMode::Expanded;
                self.hide_after = None;
            }
            EventKind::SpeechState => match event
                .payload
                .get("state")
                .and_then(serde_json::Value::as_str)
            {
                Some("listening") => {
                    self.phase = Phase::Listening;
                    self.headline = "Listening".into();
                    self.detail = "Caps to finish · processed locally".into();
                    self.show_capsule();
                }
                Some("transcribing") => {
                    self.phase = Phase::Transcribing;
                    self.headline = "Transcribing".into();
                    self.detail = "Finishing your thought locally".into();
                    self.show_capsule();
                }
                Some("speaking") => {
                    self.phase = Phase::Speaking;
                    self.show_capsule();
                }
                Some("idle") if self.surface == SurfaceMode::Capsule => {
                    self.hide_after = Some(Instant::now() + Duration::from_secs(4));
                }
                _ => {}
            },
            EventKind::DesktopControlState => {
                if event.payload.get("textPrompt").is_some() {
                    self.surface = SurfaceMode::Prompt;
                    self.prompt.clear();
                    self.hide_after = None;
                } else {
                    match event
                        .payload
                        .get("shell")
                        .and_then(serde_json::Value::as_str)
                    {
                        Some("toggle") => self.toggle_expanded(),
                        Some("collapse") => self.collapse(),
                        _ => {}
                    }
                }
            }
            EventKind::HardStop => {
                self.phase = Phase::Stopped;
                self.headline = "Arc stopped".into();
                self.detail = "Voice, generation, tools, and desktop control are off".into();
                self.surface = SurfaceMode::Capsule;
                self.confirmation = None;
                self.hide_after = Some(Instant::now() + Duration::from_secs(4));
            }
            EventKind::ServiceHealth
                if event
                    .payload
                    .get("healthy")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false) =>
            {
                self.phase = Phase::Error;
                self.headline = "Arc needs attention".into();
                self.detail = event
                    .payload
                    .get("service")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("A service")
                    .to_owned()
                    + " is unavailable";
                self.surface = SurfaceMode::Capsule;
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) {
        if self
            .hide_after
            .is_some_and(|deadline| Instant::now() >= deadline)
            && self.surface == SurfaceMode::Capsule
        {
            self.surface = SurfaceMode::Hidden;
            self.phase = Phase::Idle;
            self.hide_after = None;
        }
    }

    pub fn toggle_expanded(&mut self) {
        self.surface = if self.surface == SurfaceMode::Expanded {
            if self.phase == Phase::Idle {
                SurfaceMode::Hidden
            } else {
                SurfaceMode::Capsule
            }
        } else {
            SurfaceMode::Expanded
        };
        self.hide_after = None;
    }

    pub fn collapse(&mut self) {
        self.surface = SurfaceMode::Hidden;
        self.hide_after = None;
    }

    pub fn show_capsule(&mut self) {
        if self.surface == SurfaceMode::Hidden {
            self.surface = SurfaceMode::Capsule;
        }
        self.hide_after = None;
    }
}

fn text(value: &serde_json::Value) -> Option<String> {
    let payload = value
        .get("hermes")
        .and_then(|h| h.get("payload"))
        .unwrap_or(value);
    ["text", "delta", "message", "summary"]
        .iter()
        .find_map(|key| payload.get(key).and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn task_detail(event: &Event) -> String {
    if let Some(route) = event.payload.get("route") {
        return format!(
            "{} · {}",
            route
                .get("model")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Luna"),
            route
                .get("effort")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("medium")
        );
    }
    event
        .payload
        .get("state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("In progress")
        .replace('_', " ")
}

fn truncate(text: &mut String, length: usize) {
    if text.chars().count() > length {
        *text = text.chars().take(length).collect::<String>() + "…";
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: EventKind, payload: serde_json::Value) -> Event {
        Event {
            sequence: 1,
            at: String::new(),
            kind,
            payload,
        }
    }

    #[test]
    fn idle_is_invisible_and_voice_uses_a_capsule() {
        let mut state = ShellState::default();
        assert_eq!(state.surface, SurfaceMode::Hidden);
        state.handle(&event(EventKind::SpeechState, json!({"state":"listening"})));
        assert_eq!(state.surface, SurfaceMode::Capsule);
        assert_eq!(state.phase, Phase::Listening);
    }

    #[test]
    fn toggle_expands_without_becoming_an_application() {
        let mut state = ShellState::default();
        state.toggle_expanded();
        assert_eq!(state.surface, SurfaceMode::Expanded);
        state.collapse();
        assert_eq!(state.surface, SurfaceMode::Hidden);
    }

    #[test]
    fn confirmation_expands_and_does_not_auto_hide() {
        let mut state = ShellState::default();
        state.handle(&event(
            EventKind::ConfirmationRequested,
            json!({"summary":"Send it?"}),
        ));
        assert_eq!(state.surface, SurfaceMode::Expanded);
        assert_eq!(state.phase, Phase::NeedsAttention);
    }
}
