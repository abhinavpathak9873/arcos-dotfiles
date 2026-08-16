use arc_protocol::{ActionReceipt, ActivityItem, ActivityKind, ActivityState, EventKind};
use chrono::Utc;
use serde_json::Value;
use std::{fs, path::PathBuf};
use uuid::Uuid;

const MAX_ITEMS: usize = 200;

pub struct ActivityStore {
    path: PathBuf,
    items: Vec<ActivityItem>,
}

impl ActivityStore {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        let items = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            Vec::new()
        };
        Ok(Self { path, items })
    }

    pub fn items(&self, limit: usize) -> &[ActivityItem] {
        let start = self.items.len().saturating_sub(limit.clamp(1, MAX_ITEMS));
        &self.items[start..]
    }

    pub fn user_message(&mut self, id: Uuid, text: &str) -> anyhow::Result<()> {
        if self.items.iter().any(|item| item.id == id) {
            return Ok(());
        }
        self.push(ActivityItem {
            id,
            at: Utc::now().to_rfc3339(),
            kind: ActivityKind::UserMessage,
            title: "You".into(),
            body: text.into(),
            state: ActivityState::Complete,
            task_id: Some(id.to_string()),
            receipt_id: None,
            source_uri: None,
            metadata: Value::Null,
        })
    }

    pub fn record_event(&mut self, kind: &EventKind, payload: &Value) -> anyhow::Result<()> {
        if matches!(
            kind,
            EventKind::AssistantTextDelta | EventKind::AssistantTextComplete
        ) {
            let text = event_text(payload).unwrap_or_default();
            if let Some(item) = self.items.iter_mut().rev().find(|item| {
                item.kind == ActivityKind::AssistantMessage && item.state == ActivityState::Active
            }) {
                if !text.is_empty() {
                    if matches!(kind, EventKind::AssistantTextDelta) {
                        item.body.push_str(&text);
                    } else if item.body.is_empty() {
                        item.body = text;
                    }
                }
                if matches!(kind, EventKind::AssistantTextComplete) {
                    item.state = ActivityState::Complete;
                }
                return self.save();
            }
        }

        let Some((activity_kind, title, state, body)) = project(kind, payload) else {
            return Ok(());
        };
        self.push(ActivityItem {
            id: Uuid::new_v4(),
            at: Utc::now().to_rfc3339(),
            kind: activity_kind,
            title,
            body,
            state,
            task_id: payload
                .get("taskId")
                .or_else(|| payload.get("utteranceId"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            receipt_id: payload
                .get("receiptId")
                .and_then(Value::as_str)
                .and_then(|id| id.parse().ok()),
            source_uri: payload
                .get("sourceUri")
                .and_then(Value::as_str)
                .map(str::to_owned),
            metadata: payload.clone(),
        })
    }

    pub fn receipt(&mut self, receipt: &ActionReceipt) -> anyhow::Result<()> {
        self.push(ActivityItem {
            id: Uuid::new_v4(),
            at: receipt.at.clone(),
            kind: ActivityKind::Receipt,
            title: receipt.action.replace('_', " "),
            body: receipt.detail.clone(),
            state: match receipt.outcome {
                arc_protocol::ActionOutcome::Succeeded => ActivityState::Complete,
                arc_protocol::ActionOutcome::Failed => ActivityState::Failed,
                arc_protocol::ActionOutcome::Cancelled => ActivityState::Cancelled,
                arc_protocol::ActionOutcome::Blocked => ActivityState::NeedsAttention,
            },
            task_id: None,
            receipt_id: Some(receipt.id),
            source_uri: None,
            metadata: serde_json::to_value(receipt)?,
        })
    }

    fn push(&mut self, item: ActivityItem) -> anyhow::Result<()> {
        self.items.push(item);
        if self.items.len() > MAX_ITEMS {
            self.items.drain(..self.items.len() - MAX_ITEMS);
        }
        self.save()
    }

    fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.items)?)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

fn project(
    kind: &EventKind,
    payload: &Value,
) -> Option<(ActivityKind, String, ActivityState, String)> {
    let result = match kind {
        EventKind::TranscriptStable => (
            ActivityKind::Transcript,
            "You said".into(),
            ActivityState::Complete,
            event_text(payload).unwrap_or_default(),
        ),
        EventKind::AssistantTextDelta => (
            ActivityKind::AssistantMessage,
            "Arc".into(),
            ActivityState::Active,
            event_text(payload).unwrap_or_default(),
        ),
        EventKind::AssistantTextComplete => (
            ActivityKind::AssistantMessage,
            "Arc".into(),
            ActivityState::Complete,
            event_text(payload).unwrap_or_default(),
        ),
        EventKind::TaskProgress => (
            ActivityKind::Task,
            "Task".into(),
            if payload.get("state").and_then(Value::as_str) == Some("cancelled") {
                ActivityState::Cancelled
            } else {
                ActivityState::Active
            },
            payload
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("Working")
                .into(),
        ),
        EventKind::ToolCall => (
            ActivityKind::Tool,
            "Desktop action".into(),
            ActivityState::Active,
            event_text(payload).unwrap_or_else(|| "Using a tool".into()),
        ),
        EventKind::ConfirmationRequested => (
            ActivityKind::Confirmation,
            "Confirmation needed".into(),
            ActivityState::NeedsAttention,
            payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("Review this action")
                .into(),
        ),
        EventKind::HardStop => (
            ActivityKind::System,
            "Stopped".into(),
            ActivityState::Cancelled,
            "Microphone, speech, generation, tools, and desktop control stopped.".into(),
        ),
        _ => return None,
    };
    Some(result)
}

fn event_text(value: &Value) -> Option<String> {
    let payload = value
        .get("hermes")
        .and_then(|h| h.get("payload"))
        .unwrap_or(value);
    ["text", "delta", "message", "summary"]
        .iter()
        .find_map(|key| payload.get(key).and_then(Value::as_str))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projection_persists_and_coalesces_streamed_assistant_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("activity.json");
        let mut store = ActivityStore::open(path.clone()).unwrap();
        store.user_message(Uuid::new_v4(), "hello").unwrap();
        store
            .record_event(&EventKind::AssistantTextDelta, &json!({"text":"Hi"}))
            .unwrap();
        store
            .record_event(&EventKind::AssistantTextDelta, &json!({"text":" there"}))
            .unwrap();
        store
            .record_event(&EventKind::AssistantTextComplete, &json!({}))
            .unwrap();
        let restored = ActivityStore::open(path).unwrap();
        assert_eq!(restored.items(10).len(), 2);
        assert_eq!(restored.items(10)[1].body, "Hi there");
        assert_eq!(restored.items(10)[1].state, ActivityState::Complete);
    }
}
