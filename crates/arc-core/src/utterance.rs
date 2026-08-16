use arc_protocol::ModelRoute;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionState {
    Accepted,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtteranceRecord {
    pub id: Uuid,
    pub text: String,
    pub accepted_at: String,
    pub state: SubmissionState,
    pub route: ModelRoute,
    pub room_id: Option<Uuid>,
    pub hermes_session_id: Option<String>,
    pub codex_thread_id: Option<String>,
    #[serde(default)]
    pub result: Value,
}

#[derive(Default, Serialize, Deserialize)]
struct Data {
    utterances: BTreeMap<Uuid, UtteranceRecord>,
}

pub struct UtteranceStore {
    path: PathBuf,
    data: Data,
}

impl UtteranceStore {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_owned();
        let data = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            Data::default()
        };
        Ok(Self { path, data })
    }

    /// Returns the original record for duplicate UUIDs. A UUID may never be
    /// reused with different text; treating that as success could execute an
    /// unrelated request exactly once under the wrong identity.
    pub fn accept(&mut self, record: UtteranceRecord) -> anyhow::Result<(UtteranceRecord, bool)> {
        if let Some(existing) = self.data.utterances.get(&record.id) {
            anyhow::ensure!(
                existing.text == record.text,
                "utterance UUID was reused with different text"
            );
            return Ok((existing.clone(), true));
        }
        self.data.utterances.insert(record.id, record.clone());
        self.persist()?;
        Ok((record, false))
    }

    pub fn cancel_active(&mut self) -> anyhow::Result<usize> {
        let mut count = 0;
        for record in self.data.utterances.values_mut() {
            if matches!(
                record.state,
                SubmissionState::Accepted | SubmissionState::Running
            ) {
                record.state = SubmissionState::Cancelled;
                count += 1;
            }
        }
        if count > 0 {
            self.persist()?;
        }
        Ok(count)
    }

    pub fn list(&self) -> Vec<UtteranceRecord> {
        self.data.utterances.values().rev().cloned().collect()
    }

    fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.data)?)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

impl UtteranceRecord {
    pub fn accepted(
        id: Uuid,
        text: String,
        route: ModelRoute,
        room_id: Option<Uuid>,
        hermes_session_id: Option<String>,
    ) -> Self {
        Self {
            id,
            text,
            accepted_at: Utc::now().to_rfc3339(),
            state: SubmissionState::Accepted,
            route,
            room_id,
            hermes_session_id,
            codex_thread_id: None,
            result: Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_router;

    #[test]
    fn duplicate_survives_restart_and_is_not_reaccepted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("utterances.json");
        let id = Uuid::new_v4();
        let record = UtteranceRecord::accepted(
            id,
            "hello".into(),
            model_router::route("hello", None, None),
            None,
            None,
        );
        let mut store = UtteranceStore::open(&path).unwrap();
        assert!(!store.accept(record.clone()).unwrap().1);
        drop(store);
        let mut restored = UtteranceStore::open(path).unwrap();
        assert!(restored.accept(record).unwrap().1);
    }

    #[test]
    fn uuid_reuse_with_different_text_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = UtteranceStore::open(directory.path().join("utterances.json")).unwrap();
        let id = Uuid::new_v4();
        store
            .accept(UtteranceRecord::accepted(
                id,
                "first".into(),
                model_router::route("first", None, None),
                None,
                None,
            ))
            .unwrap();
        assert!(store
            .accept(UtteranceRecord::accepted(
                id,
                "second".into(),
                model_router::route("second", None, None),
                None,
                None
            ))
            .is_err());
    }
}
