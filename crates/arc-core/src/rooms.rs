use arc_protocol::{EvidenceItem, EvidenceKind, ProjectRoom, RoomResource, TimelineEvent};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Default)]
struct RoomData {
    rooms: BTreeMap<Uuid, ProjectRoom>,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    #[error("project room not found")]
    NotFound,
    #[error("project room name cannot be empty")]
    EmptyName,
    #[error("evidence must have a title and URI")]
    InvalidEvidence,
    #[error("room storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("invalid room store: {0}")]
    Invalid(#[from] serde_json::Error),
}

pub struct RoomStore {
    path: PathBuf,
    data: RoomData,
}

pub struct EvidenceInput {
    pub kind: EvidenceKind,
    pub title: String,
    pub uri: String,
    pub source_uri: Option<String>,
    pub claim: Option<String>,
    pub metadata: Value,
}

impl RoomStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RoomError> {
        let path = path.as_ref().to_owned();
        let data = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            RoomData::default()
        };
        Ok(Self { path, data })
    }

    pub fn list(&self, include_archived: bool) -> Vec<ProjectRoom> {
        let mut rooms: Vec<_> = self
            .data
            .rooms
            .values()
            .filter(|room| include_archived || !room.archived)
            .cloned()
            .collect();
        rooms.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        rooms
    }

    pub fn get(&self, id: Uuid) -> Result<&ProjectRoom, RoomError> {
        self.data.rooms.get(&id).ok_or(RoomError::NotFound)
    }

    pub fn create(&mut self, name: String) -> Result<ProjectRoom, RoomError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RoomError::EmptyName);
        }
        let now = Utc::now().to_rfc3339();
        let room = ProjectRoom {
            id: Uuid::new_v4(),
            name: name.into(),
            created_at: now.clone(),
            updated_at: now.clone(),
            archived: false,
            sway_layout: None,
            resources: vec![],
            evidence: vec![],
            timeline: vec![TimelineEvent {
                id: Uuid::new_v4(),
                at: now,
                actor: "user".into(),
                action: "room.created".into(),
                summary: format!("Created project room {name}"),
                receipt_id: None,
                metadata: Value::Null,
            }],
        };
        self.data.rooms.insert(room.id, room.clone());
        self.persist()?;
        Ok(room)
    }

    pub fn add_resource(
        &mut self,
        room_id: Uuid,
        resource: RoomResource,
    ) -> Result<ProjectRoom, RoomError> {
        let room = self
            .data
            .rooms
            .get_mut(&room_id)
            .ok_or(RoomError::NotFound)?;
        room.resources.push(resource.clone());
        touch(room);
        room.timeline.push(TimelineEvent {
            id: Uuid::new_v4(),
            at: Utc::now().to_rfc3339(),
            actor: "arc".into(),
            action: "resource.added".into(),
            summary: format!("Added {}", resource.title),
            receipt_id: None,
            metadata: serde_json::json!({ "kind": resource.kind, "uri": resource.uri }),
        });
        let result = room.clone();
        self.persist()?;
        Ok(result)
    }

    pub fn add_evidence(
        &mut self,
        room_id: Uuid,
        input: EvidenceInput,
    ) -> Result<EvidenceItem, RoomError> {
        if input.title.trim().is_empty() || input.uri.trim().is_empty() {
            return Err(RoomError::InvalidEvidence);
        }
        let room = self
            .data
            .rooms
            .get_mut(&room_id)
            .ok_or(RoomError::NotFound)?;
        let evidence = EvidenceItem {
            id: Uuid::new_v4(),
            kind: input.kind,
            title: input.title.trim().into(),
            uri: input.uri.trim().into(),
            source_uri: input.source_uri,
            claim: input.claim,
            captured_at: Utc::now().to_rfc3339(),
            metadata: input.metadata,
        };
        room.evidence.push(evidence.clone());
        touch(room);
        room.timeline.push(TimelineEvent {
            id: Uuid::new_v4(),
            at: Utc::now().to_rfc3339(),
            actor: "arc".into(),
            action: "evidence.added".into(),
            summary: format!("Added evidence: {}", evidence.title),
            receipt_id: None,
            metadata: serde_json::json!({ "evidence_id": evidence.id }),
        });
        self.persist()?;
        Ok(evidence)
    }

    pub fn record_event(
        &mut self,
        room_id: Uuid,
        actor: String,
        action: String,
        summary: String,
        receipt_id: Option<Uuid>,
        metadata: Value,
    ) -> Result<TimelineEvent, RoomError> {
        let room = self
            .data
            .rooms
            .get_mut(&room_id)
            .ok_or(RoomError::NotFound)?;
        let event = TimelineEvent {
            id: Uuid::new_v4(),
            at: Utc::now().to_rfc3339(),
            actor,
            action,
            summary,
            receipt_id,
            metadata,
        };
        room.timeline.push(event.clone());
        touch(room);
        self.persist()?;
        Ok(event)
    }

    pub fn snapshot_layout(&mut self, room_id: Uuid, layout: Value) -> Result<(), RoomError> {
        let room = self
            .data
            .rooms
            .get_mut(&room_id)
            .ok_or(RoomError::NotFound)?;
        room.sway_layout = Some(layout);
        touch(room);
        self.persist()
    }

    pub fn archive(&mut self, room_id: Uuid, archived: bool) -> Result<(), RoomError> {
        let room = self
            .data
            .rooms
            .get_mut(&room_id)
            .ok_or(RoomError::NotFound)?;
        room.archived = archived;
        touch(room);
        self.persist()
    }

    fn persist(&self) -> Result<(), RoomError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.data)?)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

fn touch(room: &mut ProjectRoom) {
    room.updated_at = Utc::now().to_rfc3339();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_and_evidence_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rooms.json");
        let room_id;
        {
            let mut rooms = RoomStore::open(&path).unwrap();
            let room = rooms.create("ArcOS".into()).unwrap();
            room_id = room.id;
            rooms
                .add_evidence(
                    room.id,
                    EvidenceInput {
                        kind: EvidenceKind::Webpage,
                        title: "Sway documentation".into(),
                        uri: "https://example.test/sway".into(),
                        source_uri: Some("https://example.test/sway".into()),
                        claim: Some("Seats have independent focus".into()),
                        metadata: Value::Null,
                    },
                )
                .unwrap();
        }
        let rooms = RoomStore::open(path).unwrap();
        let restored = rooms.get(room_id).unwrap();
        assert_eq!(restored.name, "ArcOS");
        assert_eq!(restored.evidence.len(), 1);
        assert_eq!(restored.timeline.len(), 2);
    }

    #[test]
    fn archived_rooms_are_hidden_by_default() {
        let directory = tempfile::tempdir().unwrap();
        let mut rooms = RoomStore::open(directory.path().join("rooms.json")).unwrap();
        let room = rooms.create("Old".into()).unwrap();
        rooms.archive(room.id, true).unwrap();
        assert!(rooms.list(false).is_empty());
        assert_eq!(rooms.list(true).len(), 1);
    }
}
