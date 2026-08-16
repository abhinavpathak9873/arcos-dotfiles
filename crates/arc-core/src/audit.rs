use arc_protocol::{ActionOutcome, ActionReceipt, PermissionDecision};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ReceiptInput {
    pub actor: String,
    pub action: String,
    pub target: String,
    pub outcome: ActionOutcome,
    pub reversible: bool,
    pub permission: PermissionDecision,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("invalid audit receipt: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error("audit chain is invalid at sequence {0}")]
    Chain(u64),
}

pub struct AuditStore {
    path: PathBuf,
    receipts: Vec<ActionReceipt>,
}

#[derive(Serialize)]
struct HashMaterial<'a> {
    id: Uuid,
    sequence: u64,
    at: &'a str,
    actor: &'a str,
    action: &'a str,
    target: &'a str,
    outcome: &'a ActionOutcome,
    reversible: bool,
    permission: &'a PermissionDecision,
    detail: &'a str,
    previous_hash: &'a Option<String>,
}

impl AuditStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AuditError> {
        let path = path.as_ref().to_owned();
        let receipts = if path.exists() {
            let reader = BufReader::new(fs::File::open(&path)?);
            reader
                .lines()
                .filter(|line| line.as_ref().map_or(true, |value| !value.trim().is_empty()))
                .map(|line| Ok(serde_json::from_str::<ActionReceipt>(&line?)?))
                .collect::<Result<Vec<_>, AuditError>>()?
        } else {
            vec![]
        };
        let store = Self { path, receipts };
        store.verify()?;
        Ok(store)
    }

    pub fn append(&mut self, input: ReceiptInput) -> Result<ActionReceipt, AuditError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut receipt = ActionReceipt {
            id: Uuid::new_v4(),
            sequence: self.receipts.len() as u64 + 1,
            at: Utc::now().to_rfc3339(),
            actor: input.actor,
            action: input.action,
            target: input.target,
            outcome: input.outcome,
            reversible: input.reversible,
            permission: input.permission,
            detail: input.detail,
            previous_hash: self.receipts.last().map(|item| item.hash.clone()),
            hash: String::new(),
        };
        receipt.hash = receipt_hash(&receipt);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, &receipt)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }

    pub fn receipts(&self) -> &[ActionReceipt] {
        &self.receipts
    }

    pub fn verify(&self) -> Result<(), AuditError> {
        let mut previous: Option<&str> = None;
        for (index, receipt) in self.receipts.iter().enumerate() {
            let expected_sequence = index as u64 + 1;
            if receipt.sequence != expected_sequence
                || receipt.previous_hash.as_deref() != previous
                || receipt.hash != receipt_hash(receipt)
            {
                return Err(AuditError::Chain(receipt.sequence));
            }
            previous = Some(&receipt.hash);
        }
        Ok(())
    }
}

fn receipt_hash(receipt: &ActionReceipt) -> String {
    let material = HashMaterial {
        id: receipt.id,
        sequence: receipt.sequence,
        at: &receipt.at,
        actor: &receipt.actor,
        action: &receipt.action,
        target: &receipt.target,
        outcome: &receipt.outcome,
        reversible: receipt.reversible,
        permission: &receipt.permission,
        detail: &receipt.detail,
        previous_hash: &receipt.previous_hash,
    };
    let bytes = serde_json::to_vec(&material).expect("hash material is serializable");
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(target: &str) -> ReceiptInput {
        ReceiptInput {
            actor: "arc".into(),
            action: "apps.launch".into(),
            target: target.into(),
            outcome: ActionOutcome::Succeeded,
            reversible: true,
            permission: PermissionDecision::Allowed,
            detail: "spawned".into(),
        }
    }

    #[test]
    fn persists_and_verifies_hash_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipts.jsonl");
        let mut store = AuditStore::open(&path).unwrap();
        let first = store.append(input("chrome")).unwrap();
        let second = store.append(input("dolphin")).unwrap();
        assert_eq!(second.previous_hash.as_deref(), Some(first.hash.as_str()));
        let reopened = AuditStore::open(path).unwrap();
        assert_eq!(reopened.receipts().len(), 2);
        reopened.verify().unwrap();
    }

    #[test]
    fn detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipts.jsonl");
        let mut store = AuditStore::open(&path).unwrap();
        store.append(input("chrome")).unwrap();
        let changed = fs::read_to_string(&path)
            .unwrap()
            .replace("chrome", "other");
        fs::write(&path, changed).unwrap();
        assert!(matches!(AuditStore::open(path), Err(AuditError::Chain(1))));
    }
}
