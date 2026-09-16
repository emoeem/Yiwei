//! Sync operations and idempotent ingestion (design report §16.5, §21.3).
//!
//! An [`Op`] is the unit that travels between devices. Applying it is
//! idempotent because operations are keyed by `opId`: pushing the same batch
//! twice — which happens after every retry — must not change any state.
//!
//! [`OpLog`] is the shared merge engine. The client applies its own operations
//! to it and the server applies received ones to the *same* implementation, so
//! the contract tests here cover both sides.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::SyncError;
use crate::hlc::{DeviceId, Hlc};
use crate::replica::{Fields, Replica};

/// Entity families that participate in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// Book metadata.
    Publication,
    /// Original / translated / bilingual edition.
    Edition,
    /// Highlight, note, bookmark, …
    Annotation,
    /// Note body revisions.
    Note,
    /// Reading progress.
    Progress,
    /// Settings.
    Setting,
    /// Collections and tags.
    Collection,
    /// Wenyi glossary entries.
    Glossary,
}

/// What an operation does to a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Create or update fields.
    Upsert,
    /// Delete the row (tombstone).
    Delete,
    /// Explicitly bring a deleted row back (reincarnation).
    Restore,
}

/// One synchronised operation (the `ops[]` element of §16.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Op {
    /// Idempotency key, UUIDv7 in practice.
    pub op_id: String,
    /// Entity family.
    pub entity: EntityKind,
    /// Row identity.
    pub entity_id: String,
    /// Operation kind.
    #[serde(rename = "op")]
    pub kind: OpKind,
    /// Revision the client saw before writing, used to detect concurrency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<i64>,
    /// Field payload with per-field clocks.
    #[serde(default)]
    pub fields: Fields,
    /// Clock of the operation itself.
    pub hlc: Hlc,
    /// Device that produced the operation.
    pub device_id: DeviceId,
    /// Optional integrity hash of `fields`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
}

impl Op {
    /// Integrity hash of this operation's payload.
    pub fn computed_payload_hash(&self) -> String {
        payload_hash(&self.fields)
    }

    /// Validate the operation before it is applied or forwarded.
    ///
    /// Nothing here is optional: an operation whose device, clocks or payload
    /// hash do not line up is rejected instead of being merged on trust.
    pub fn validate(&self) -> Result<(), SyncError> {
        if self.op_id.is_empty() {
            return Err(SyncError::InvalidOp("op_id must not be empty".into()));
        }
        if self.entity_id.is_empty() {
            return Err(SyncError::InvalidOp("entity_id must not be empty".into()));
        }
        for (key, envelope) in &self.fields {
            envelope.validate(key)?;
            if envelope.t.device() != &self.device_id {
                return Err(SyncError::OpDeviceMismatch {
                    op_device: self.device_id.to_string(),
                    field_device: envelope.t.device().to_string(),
                });
            }
        }
        if let Some(declared) = &self.payload_hash {
            let computed = self.computed_payload_hash();
            if declared != &computed {
                return Err(SyncError::PayloadHashMismatch {
                    declared: declared.clone(),
                    computed,
                });
            }
        }
        Ok(())
    }
}

/// Canonical hash of a field payload.
pub fn payload_hash(fields: &Fields) -> String {
    // `Fields` is a `BTreeMap`, so serialization order is deterministic.
    let bytes = serde_json::to_vec(fields).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// What happened to one operation.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyOutcome {
    /// The operation changed state.
    Applied,
    /// Already seen; nothing changed (the normal retry path).
    Duplicate,
    /// Rejected with a reason.
    Rejected(SyncError),
}

/// One operation stored in the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredOp {
    /// Monotonic sequence number, used as the pull cursor.
    pub sequence: u64,
    /// The operation itself.
    pub op: Op,
}

/// Result of applying a batch of operations.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PushOutcome {
    /// Per-operation outcome, in input order.
    pub results: Vec<(String, ApplyOutcome)>,
    /// Number of operations applied.
    pub applied: usize,
    /// Number of duplicates ignored.
    pub duplicates: usize,
    /// Number of rejected operations.
    pub rejected: usize,
}

/// A pull request as sent by a client (§16.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    /// Requesting device.
    pub device_id: DeviceId,
    /// Opaque cursor returned by a previous pull.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Maximum number of operations to return.
    pub limit: u32,
}

/// Result of a pull request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    /// Operations after the requested cursor.
    pub ops: Vec<StoredOp>,
    /// Cursor to send with the next pull.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Whether more operations are waiting.
    pub has_more: bool,
}

/// An operation log plus the replica state it produced.
#[derive(Debug, Default, Clone)]
pub struct OpLog {
    seen: BTreeSet<String>,
    stored: Vec<StoredOp>,
    replicas: BTreeMap<(EntityKind, String), Replica>,
}

impl OpLog {
    /// An empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one operation.
    pub fn apply(&mut self, op: &Op) -> ApplyOutcome {
        if let Err(error) = op.validate() {
            return ApplyOutcome::Rejected(error);
        }
        if self.seen.contains(&op.op_id) {
            return ApplyOutcome::Duplicate;
        }
        if let Err(error) = self.apply_inner(op) {
            return ApplyOutcome::Rejected(error);
        }
        self.seen.insert(op.op_id.clone());
        self.stored.push(StoredOp {
            sequence: self.stored.len() as u64 + 1,
            op: op.clone(),
        });
        ApplyOutcome::Applied
    }

    /// Apply a batch, collecting per-operation outcomes.
    pub fn apply_push(&mut self, ops: &[Op]) -> PushOutcome {
        let mut outcome = PushOutcome::default();
        for op in ops {
            let result = self.apply(op);
            match &result {
                ApplyOutcome::Applied => outcome.applied += 1,
                ApplyOutcome::Duplicate => outcome.duplicates += 1,
                ApplyOutcome::Rejected(_) => outcome.rejected += 1,
            }
            outcome.results.push((op.op_id.clone(), result));
        }
        outcome
    }

    /// Read operations after `cursor` (§16.5 pull).
    pub fn pull(&self, request: &PullRequest) -> Result<PullResponse, SyncError> {
        let after = match &request.cursor {
            None => 0,
            Some(cursor) => cursor
                .parse::<u64>()
                .map_err(|_| SyncError::InvalidOp(format!("unusable pull cursor {cursor:?}")))?,
        };
        let limit = request.limit.max(1) as usize;
        let ops: Vec<StoredOp> = self
            .stored
            .iter()
            .filter(|stored| stored.sequence > after)
            .take(limit)
            .cloned()
            .collect();
        let has_more = ops
            .last()
            .is_some_and(|last| self.stored.len() as u64 > last.sequence);
        let next_cursor = ops.last().map(|last| last.sequence.to_string());
        Ok(PullResponse {
            ops,
            next_cursor,
            has_more,
        })
    }

    /// Current merged state of a row.
    pub fn replica(&self, entity: EntityKind, entity_id: &str) -> Option<&Replica> {
        self.replicas.get(&(entity, entity_id.to_string()))
    }

    /// Number of operations stored.
    pub fn op_count(&self) -> usize {
        self.stored.len()
    }

    /// Number of rows known to the log.
    pub fn replica_count(&self) -> usize {
        self.replicas.len()
    }

    fn apply_inner(&mut self, op: &Op) -> Result<(), SyncError> {
        let key = (op.entity, op.entity_id.clone());
        let replica = self
            .replicas
            .entry(key)
            .or_insert_with(|| Replica::new(op.hlc.clone()));
        match op.kind {
            OpKind::Upsert => {
                for (key, envelope) in &op.fields {
                    replica.set_field(key, envelope.v.clone(), envelope.t.clone());
                }
                // A row that only ever carried timestamps still needs its clock.
                if op.fields.is_empty() {
                    replica.updated_at = replica.updated_at.clone().max(op.hlc.clone());
                }
            }
            OpKind::Delete => {
                replica.tombstone(op.hlc.clone());
            }
            OpKind::Restore => {
                replica.restore(&op.op_id, op.hlc.clone())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::FieldEnvelope;

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("device")
    }

    fn hlc(physical_ms: u64, dev: &str) -> Hlc {
        Hlc::new(physical_ms, 0, device(dev))
    }

    fn field(value: &str, physical_ms: u64, dev: &str) -> FieldEnvelope<serde_json::Value> {
        FieldEnvelope::new(serde_json::json!(value), hlc(physical_ms, dev))
    }

    fn upsert(op_id: &str, dev: &str, physical_ms: u64, fields: Fields) -> Op {
        Op {
            op_id: op_id.into(),
            entity: EntityKind::Annotation,
            entity_id: "annotation-1".into(),
            kind: OpKind::Upsert,
            base_revision: None,
            fields,
            hlc: hlc(physical_ms, dev),
            device_id: device(dev),
            payload_hash: None,
        }
    }

    #[test]
    fn duplicate_operations_change_nothing() {
        let mut log = OpLog::new();
        let mut fields = Fields::new();
        fields.insert("note".into(), field("hello", 10, "dev-a"));
        let op = upsert("op-1", "dev-a", 10, fields);

        assert_eq!(log.apply(&op), ApplyOutcome::Applied);
        assert_eq!(log.apply(&op), ApplyOutcome::Duplicate);
        assert_eq!(log.op_count(), 1);

        let outcome = log.apply_push(std::slice::from_ref(&op));
        assert_eq!(outcome.duplicates, 1);
        assert_eq!(outcome.applied, 0);
    }

    #[test]
    fn out_of_order_delivery_converges() {
        let mut first_fields = Fields::new();
        first_fields.insert("note".into(), field("first", 10, "dev-a"));
        let first = upsert("op-1", "dev-a", 10, first_fields);

        let mut second_fields = Fields::new();
        second_fields.insert("note".into(), field("second", 20, "dev-b"));
        let second = upsert("op-2", "dev-b", 20, second_fields);

        let mut ordered = OpLog::new();
        ordered.apply_push(&[first.clone(), second.clone()]);
        let mut reversed = OpLog::new();
        reversed.apply_push(&[second, first]);

        assert_eq!(
            ordered.replica(EntityKind::Annotation, "annotation-1"),
            reversed.replica(EntityKind::Annotation, "annotation-1")
        );
        assert_eq!(
            ordered
                .replica(EntityKind::Annotation, "annotation-1")
                .expect("replica")
                .fields["note"]
                .v,
            serde_json::json!("second")
        );
    }

    #[test]
    fn stale_fields_are_ignored_but_newer_ones_win() {
        let mut log = OpLog::new();
        let mut fields = Fields::new();
        fields.insert("note".into(), field("new", 100, "dev-a"));
        fields.insert("color".into(), field("yellow", 5, "dev-a"));
        log.apply_push(&[upsert("op-1", "dev-a", 100, fields)]);

        let mut stale = Fields::new();
        stale.insert("note".into(), field("old", 50, "dev-b"));
        log.apply_push(&[upsert("op-2", "dev-b", 50, stale)]);

        let replica = log
            .replica(EntityKind::Annotation, "annotation-1")
            .expect("replica");
        assert_eq!(replica.fields["note"].v, serde_json::json!("new"));
        assert_eq!(replica.fields["color"].v, serde_json::json!("yellow"));
    }

    #[test]
    fn delete_then_explicit_restore_via_operations() {
        let mut log = OpLog::new();
        let mut fields = Fields::new();
        fields.insert("note".into(), field("keep me", 10, "dev-a"));
        log.apply(&upsert("op-1", "dev-a", 10, fields));

        let delete = Op {
            op_id: "op-2".into(),
            entity: EntityKind::Annotation,
            entity_id: "annotation-1".into(),
            kind: OpKind::Delete,
            base_revision: None,
            fields: Fields::new(),
            hlc: hlc(20, "dev-b"),
            device_id: device("dev-b"),
            payload_hash: None,
        };
        log.apply(&delete);
        assert!(
            log.replica(EntityKind::Annotation, "annotation-1")
                .expect("replica")
                .is_deleted()
        );

        let mut revive = delete.clone();
        revive.op_id = "op-3".into();
        revive.kind = OpKind::Restore;
        revive.hlc = hlc(30, "dev-b");
        log.apply(&revive);

        let replica = log
            .replica(EntityKind::Annotation, "annotation-1")
            .expect("replica");
        assert!(!replica.is_deleted());
        assert_eq!(replica.fields["note"].v, serde_json::json!("keep me"));

        assert_eq!(log.apply(&revive), ApplyOutcome::Duplicate);
    }

    #[test]
    fn rejects_payload_hash_and_device_mismatches() {
        let mut fields = Fields::new();
        fields.insert("note".into(), field("hello", 10, "dev-a"));
        let mut op = upsert("op-1", "dev-a", 10, fields);
        op.payload_hash = Some(op.computed_payload_hash());
        assert!(op.validate().is_ok());

        op.payload_hash = Some("deadbeef".into());
        assert!(matches!(
            op.validate().unwrap_err(),
            SyncError::PayloadHashMismatch { .. }
        ));

        let mut log = OpLog::new();
        assert!(matches!(
            log.apply(&op),
            ApplyOutcome::Rejected(SyncError::PayloadHashMismatch { .. })
        ));
        assert_eq!(log.op_count(), 0, "rejected operations are not stored");

        let mut mismatched = upsert("op-2", "dev-a", 10, Fields::new());
        let mut foreign = Fields::new();
        foreign.insert("note".into(), field("hello", 10, "dev-b"));
        mismatched.fields = foreign;
        assert!(matches!(
            mismatched.validate().unwrap_err(),
            SyncError::OpDeviceMismatch { .. }
        ));
    }

    #[test]
    fn pull_paginates_with_an_opaque_cursor() {
        let mut log = OpLog::new();
        for index in 0..5u64 {
            let mut fields = Fields::new();
            fields.insert(
                "note".into(),
                field(&format!("value-{index}"), 10 + index, "dev-a"),
            );
            log.apply(&upsert(&format!("op-{index}"), "dev-a", 10 + index, fields));
        }

        let mut cursor = None;
        let mut collected = 0usize;
        loop {
            let response = log
                .pull(&PullRequest {
                    device_id: device("dev-b"),
                    cursor: cursor.clone(),
                    limit: 2,
                })
                .expect("pull");
            collected += response.ops.len();
            cursor = response.next_cursor.clone();
            if !response.has_more {
                break;
            }
        }
        assert_eq!(collected, 5);

        let bad = log.pull(&PullRequest {
            device_id: device("dev-b"),
            cursor: Some("not-a-cursor".into()),
            limit: 2,
        });
        assert!(matches!(bad.unwrap_err(), SyncError::InvalidOp(_)));
    }

    #[test]
    fn serializes_in_the_documented_shape() {
        let json = serde_json::json!({
            "opId": "01932b39-0000-7000-8000-000000000001",
            "entity": "annotation",
            "entityId": "annotation-1",
            "op": "upsert",
            "baseRevision": 12,
            "fields": {
                "note": {
                    "v": "text",
                    "t": "000000000000000a-00000000-dev-a",
                    "s": "dev-a"
                }
            },
            "hlc": "000000000000000a-00000000-dev-a",
            "deviceId": "dev-a"
        });
        let op: Op = serde_json::from_value(json).expect("deserialize");
        assert_eq!(op.op_id, "01932b39-0000-7000-8000-000000000001");
        assert_eq!(op.kind, OpKind::Upsert);
        assert_eq!(op.base_revision, Some(12));
        assert!(op.validate().is_ok());

        let round_tripped = serde_json::to_value(&op).expect("serialize");
        assert_eq!(round_tripped["op"], serde_json::json!("upsert"));
        assert_eq!(round_tripped["entityId"], serde_json::json!("annotation-1"));
        assert!(round_tripped.get("payloadHash").is_none());
    }
}
