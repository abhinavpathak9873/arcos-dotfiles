use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Memory,
    Alias,
    Skill,
    Tool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    Draft,
    Testing,
    Active,
    Pinned,
    Disabled,
    Superseded,
    Quarantined,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub reason: String,
    pub session_id: String,
    pub excerpts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactVersion {
    pub number: u32,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub actor: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub kind: ArtifactKind,
    pub scope: String,
    pub state: ArtifactState,
    pub confidence: f32,
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub locked: bool,
    pub permissions: Vec<String>,
    pub versions: Vec<ArtifactVersion>,
}

impl Artifact {
    pub fn content(&self) -> &str {
        &self
            .versions
            .last()
            .expect("artifact always has a version")
            .content
    }
}

#[derive(Debug, Clone)]
pub struct NewArtifact {
    pub name: String,
    pub kind: ArtifactKind,
    pub scope: String,
    pub content: String,
    pub confidence: f32,
    pub provenance: Provenance,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub at: DateTime<Utc>,
    pub artifact_id: Uuid,
    pub action: String,
    pub actor: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreData {
    artifacts: BTreeMap<Uuid, Artifact>,
    audit: Vec<AuditEvent>,
    learning_paused: bool,
    global_budget: usize,
    per_scope_budget: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum LearningError {
    #[error("artifact not found")]
    NotFound,
    #[error("artifact is locked and cannot be changed by Arc")]
    Locked,
    #[error("learning is paused")]
    Paused,
    #[error("growth budget reached for {0}")]
    Budget(String),
    #[error("invalid confidence")]
    InvalidConfidence,
    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("invalid store: {0}")]
    InvalidStore(#[from] serde_json::Error),
}

pub struct LearningStore {
    path: PathBuf,
    data: StoreData,
}

impl LearningStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LearningError> {
        let path = path.as_ref().to_owned();
        let data = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            StoreData {
                artifacts: BTreeMap::new(),
                audit: vec![],
                learning_paused: false,
                global_budget: 500,
                per_scope_budget: 100,
            }
        };
        Ok(Self { path, data })
    }
    pub fn artifacts(&self) -> impl Iterator<Item = &Artifact> {
        self.data.artifacts.values()
    }
    pub fn audit(&self) -> &[AuditEvent] {
        &self.data.audit
    }
    pub fn set_paused(&mut self, paused: bool) -> Result<(), LearningError> {
        self.data.learning_paused = paused;
        self.persist()
    }
    pub fn create(&mut self, input: NewArtifact, actor: &str) -> Result<Uuid, LearningError> {
        if self.data.learning_paused && actor == "arc" {
            return Err(LearningError::Paused);
        }
        if !(0.0..=1.0).contains(&input.confidence) {
            return Err(LearningError::InvalidConfidence);
        }
        if self.data.artifacts.len() >= self.data.global_budget {
            return Err(LearningError::Budget("global".into()));
        }
        if self
            .data
            .artifacts
            .values()
            .filter(|a| {
                a.scope == input.scope
                    && !matches!(a.state, ArtifactState::Archived | ArtifactState::Superseded)
            })
            .count()
            >= self.data.per_scope_budget
        {
            return Err(LearningError::Budget(input.scope));
        }
        let now = Utc::now();
        let id = Uuid::new_v4();
        let artifact = Artifact {
            id,
            name: input.name,
            kind: input.kind,
            scope: input.scope,
            state: ArtifactState::Draft,
            confidence: input.confidence,
            provenance: input.provenance,
            created_at: now,
            last_used_at: None,
            use_count: 0,
            success_count: 0,
            failure_count: 0,
            locked: false,
            permissions: input.permissions,
            versions: vec![ArtifactVersion {
                number: 1,
                content: input.content,
                created_at: now,
                actor: actor.into(),
                reason: "created".into(),
            }],
        };
        self.data.artifacts.insert(id, artifact);
        self.record(id, "created", actor, "new learning artifact");
        self.persist()?;
        Ok(id)
    }
    pub fn update(
        &mut self,
        id: Uuid,
        content: String,
        reason: &str,
        actor: &str,
    ) -> Result<(), LearningError> {
        let artifact = self
            .data
            .artifacts
            .get_mut(&id)
            .ok_or(LearningError::NotFound)?;
        if artifact.locked && actor == "arc" {
            return Err(LearningError::Locked);
        }
        let number = artifact.versions.last().map_or(1, |v| v.number + 1);
        artifact.versions.push(ArtifactVersion {
            number,
            content,
            created_at: Utc::now(),
            actor: actor.into(),
            reason: reason.into(),
        });
        self.record(id, "updated", actor, reason);
        self.persist()
    }
    pub fn rollback(&mut self, id: Uuid, version: u32, actor: &str) -> Result<(), LearningError> {
        let artifact = self
            .data
            .artifacts
            .get(&id)
            .ok_or(LearningError::NotFound)?;
        let content = artifact
            .versions
            .iter()
            .find(|v| v.number == version)
            .ok_or(LearningError::NotFound)?
            .content
            .clone();
        self.update(
            id,
            content,
            &format!("rolled back to version {version}"),
            actor,
        )
    }
    pub fn lock(&mut self, id: Uuid, locked: bool) -> Result<(), LearningError> {
        self.data
            .artifacts
            .get_mut(&id)
            .ok_or(LearningError::NotFound)?
            .locked = locked;
        self.record(
            id,
            if locked { "locked" } else { "unlocked" },
            "user",
            "manual governance change",
        );
        self.persist()
    }
    pub fn set_state(
        &mut self,
        id: Uuid,
        state: ArtifactState,
        actor: &str,
        reason: &str,
    ) -> Result<(), LearningError> {
        let a = self
            .data
            .artifacts
            .get_mut(&id)
            .ok_or(LearningError::NotFound)?;
        if a.locked && actor == "arc" {
            return Err(LearningError::Locked);
        }
        a.state = state;
        self.record(id, "state_changed", actor, reason);
        self.persist()
    }
    pub fn delete(&mut self, id: Uuid) -> Result<(), LearningError> {
        self.data
            .artifacts
            .remove(&id)
            .ok_or(LearningError::NotFound)?;
        self.record(id, "deleted", "user", "permanent user-requested deletion");
        self.persist()
    }
    fn record(&mut self, artifact_id: Uuid, action: &str, actor: &str, reason: &str) {
        self.data.audit.push(AuditEvent {
            at: Utc::now(),
            artifact_id,
            action: action.into(),
            actor: actor.into(),
            reason: reason.into(),
        });
    }
    fn persist(&self) -> Result<(), LearningError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.data)?)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact() -> NewArtifact {
        NewArtifact {
            name: "Concise voice".into(),
            kind: ArtifactKind::Memory,
            scope: "global".into(),
            content: "Keep voice brief".into(),
            confidence: 0.95,
            provenance: Provenance {
                reason: "user correction".into(),
                session_id: "s1".into(),
                excerpts: vec!["shorter please".into()],
            },
            permissions: vec![],
        }
    }
    #[test]
    fn versions_and_rolls_back() {
        let d = tempfile::tempdir().unwrap();
        let mut s = LearningStore::open(d.path().join("store.json")).unwrap();
        let id = s.create(artifact(), "arc").unwrap();
        s.update(id, "Be extremely brief".into(), "correction", "arc")
            .unwrap();
        s.rollback(id, 1, "user").unwrap();
        let a = s.artifacts().next().unwrap();
        assert_eq!(a.content(), "Keep voice brief");
        assert_eq!(a.versions.len(), 3);
    }
    #[test]
    fn lock_blocks_arc_not_user() {
        let d = tempfile::tempdir().unwrap();
        let mut s = LearningStore::open(d.path().join("store.json")).unwrap();
        let id = s.create(artifact(), "arc").unwrap();
        s.lock(id, true).unwrap();
        assert!(matches!(
            s.update(id, "x".into(), "auto", "arc"),
            Err(LearningError::Locked)
        ));
        s.update(id, "mine".into(), "manual edit", "user").unwrap();
    }
    #[test]
    fn paused_learning_blocks_autonomous_creation() {
        let d = tempfile::tempdir().unwrap();
        let mut s = LearningStore::open(d.path().join("store.json")).unwrap();
        s.set_paused(true).unwrap();
        assert!(matches!(
            s.create(artifact(), "arc"),
            Err(LearningError::Paused)
        ));
    }
}
