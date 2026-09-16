//! Row-level replica merge with tombstones (design report §16.3, §21.4).
//!
//! Rules implemented here, exactly as specified:
//!
//! * each field keeps the value with the greater clock (field-level LWW),
//! * `deletedAt` is the maximum of both tombstones — a tombstone never
//!   disappears because of a field write,
//! * only a *strictly newer* reincarnation token may clear a tombstone,
//! * `schemaVersion` is the maximum of both sides,
//! * `updatedAt` is the maximum over every field, tombstone and row clock.
//!
//! The merge is commutative, associative and idempotent; the tests prove this
//! over randomly generated replicas, not just hand-picked examples.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::envelope::FieldEnvelope;
use crate::error::SyncError;
use crate::hlc::Hlc;

/// The field map of one row.
pub type Fields = BTreeMap<String, FieldEnvelope<serde_json::Value>>;

/// Explicit permission to bring a deleted row back to life.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reincarnation {
    /// Opaque token identifying the revival.
    pub token: String,
    /// Clock of the revival.
    pub t: Hlc,
}

/// A synchronised row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replica {
    /// Schema version of the row, `max` on merge.
    pub schema_version: u32,
    /// Field values with their clocks.
    pub fields: Fields,
    /// Tombstone clock, present once the row was deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<Hlc>,
    /// Revival marker; only a clock newer than `deleted_at` revives the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reincarnation: Option<Reincarnation>,
    /// Maximum clock seen for this row.
    pub updated_at: Hlc,
}

/// Result of [`Replica::merge`].
#[derive(Debug, Clone, PartialEq)]
pub struct MergeResult {
    /// The merged row.
    pub merged: Replica,
    /// What changed, for the audit history required by §21.8.
    pub stats: MergeStats,
}

/// Bookkeeping about a merge, never used for correctness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeStats {
    /// Fields whose winning value came from the local side.
    pub fields_from_local: usize,
    /// Fields whose winning value came from the remote side.
    pub fields_from_remote: usize,
    /// Fields that only the remote side had.
    pub fields_added: usize,
    /// Whether the merged row is deleted.
    pub tombstoned: bool,
    /// Whether the merge brought a deleted row back to life.
    pub revived: bool,
}

impl Replica {
    /// An empty row stamped with `updated_at`.
    pub fn new(updated_at: Hlc) -> Self {
        Self {
            schema_version: 1,
            fields: Fields::new(),
            deleted_at: None,
            reincarnation: None,
            updated_at,
        }
    }

    /// Store a field value if it is newer than the stored one.
    ///
    /// Writing to a tombstoned row is allowed and remembered, but it never
    /// brings the row back: that is the "field writes cannot resurrect a
    /// tombstone" rule.
    pub fn set_field(&mut self, key: &str, value: serde_json::Value, hlc: Hlc) -> bool {
        let candidate = FieldEnvelope::new(value, hlc);
        match self.fields.get(key) {
            Some(existing) if candidate.cmp_total(existing) != Ordering::Greater => false,
            _ => {
                self.fields.insert(key.to_string(), candidate);
                self.refresh_updated_at();
                true
            }
        }
    }

    /// Mark the row deleted at `hlc`.
    pub fn tombstone(&mut self, hlc: Hlc) -> bool {
        let replaced = match &self.deleted_at {
            Some(existing) => hlc > *existing,
            None => true,
        };
        if replaced {
            self.deleted_at = Some(hlc);
            self.refresh_updated_at();
        }
        replaced
    }

    /// Bring a deleted row back with an explicit token.
    ///
    /// Returns whether the row is live afterwards. The tombstone is kept for
    /// audit; it is simply superseded by a newer revival clock.
    pub fn restore(&mut self, token: &str, hlc: Hlc) -> Result<bool, SyncError> {
        if token.is_empty() {
            return Err(SyncError::InvalidOp(
                "reincarnation token must not be empty".into(),
            ));
        }
        let candidate = Reincarnation {
            token: token.to_string(),
            t: hlc,
        };
        let replace = match &self.reincarnation {
            Some(existing) => {
                candidate
                    .t
                    .cmp(&existing.t)
                    .then_with(|| candidate.token.cmp(&existing.token))
                    == Ordering::Greater
            }
            None => true,
        };
        if replace {
            self.reincarnation = Some(candidate);
            self.refresh_updated_at();
        }
        Ok(!self.is_deleted())
    }

    /// Whether the row is currently deleted.
    pub fn is_deleted(&self) -> bool {
        match (&self.deleted_at, &self.reincarnation) {
            (Some(deleted_at), Some(reincarnation)) => reincarnation.t <= *deleted_at,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    /// Merge another replica into this one (§21.4 `mergeReplica`).
    pub fn merge(&self, other: &Replica) -> MergeResult {
        let was_deleted = self.is_deleted();
        let mut merged = self.clone();
        let mut stats = MergeStats::default();

        for (key, remote) in &other.fields {
            match merged.fields.get(key) {
                Some(local) if remote.cmp_total(local) != Ordering::Greater => {
                    stats.fields_from_local += 1;
                }
                Some(_) => {
                    merged.fields.insert(key.clone(), remote.clone());
                    stats.fields_from_remote += 1;
                }
                None => {
                    merged.fields.insert(key.clone(), remote.clone());
                    stats.fields_added += 1;
                    stats.fields_from_remote += 1;
                }
            }
        }
        stats.fields_from_local += merged
            .fields
            .keys()
            .filter(|key| !other.fields.contains_key(*key))
            .count();

        merged.deleted_at = max_hlc(self.deleted_at.clone(), other.deleted_at.clone());
        merged.reincarnation =
            pick_reincarnation(self.reincarnation.clone(), other.reincarnation.clone());
        merged.schema_version = self.schema_version.max(other.schema_version);
        merged.updated_at = merged.updated_at.clone().max(other.updated_at.clone());
        merged.refresh_updated_at();

        stats.tombstoned = merged.is_deleted();
        stats.revived = was_deleted && !merged.is_deleted();
        MergeResult { merged, stats }
    }

    /// Verify that every envelope is internally consistent (§16.5).
    pub fn validate_envelopes(&self) -> Result<(), SyncError> {
        for (key, envelope) in &self.fields {
            envelope.validate(key)?;
        }
        Ok(())
    }

    fn refresh_updated_at(&mut self) {
        let mut latest = self.updated_at.clone();
        for envelope in self.fields.values() {
            latest = latest.max(envelope.t.clone());
        }
        if let Some(deleted_at) = &self.deleted_at {
            latest = latest.max(deleted_at.clone());
        }
        if let Some(reincarnation) = &self.reincarnation {
            latest = latest.max(reincarnation.t.clone());
        }
        self.updated_at = latest;
    }
}

fn max_hlc(left: Option<Hlc>, right: Option<Hlc>) -> Option<Hlc> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn pick_reincarnation(
    left: Option<Reincarnation>,
    right: Option<Reincarnation>,
) -> Option<Reincarnation> {
    match (left, right) {
        (Some(left), Some(right)) => {
            let ordering = left
                .t
                .cmp(&right.t)
                .then_with(|| left.token.cmp(&right.token));
            Some(if ordering == Ordering::Greater {
                left
            } else {
                right
            })
        }
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::DeviceId;

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("device")
    }

    fn hlc(physical_ms: u64, counter: u32, dev: &str) -> Hlc {
        Hlc::new(physical_ms, counter, device(dev))
    }

    fn row(dev: &str, updated_at: u64) -> Replica {
        Replica::new(hlc(updated_at, 0, dev))
    }

    #[test]
    fn fields_merge_independently() {
        let mut left = row("dev-a", 1);
        left.set_field("title", serde_json::json!("A title"), hlc(10, 0, "dev-a"));
        left.set_field("author", serde_json::json!("A author"), hlc(10, 0, "dev-a"));

        let mut right = row("dev-b", 1);
        right.set_field("title", serde_json::json!("A title"), hlc(10, 0, "dev-a"));
        right.set_field("author", serde_json::json!("B author"), hlc(11, 0, "dev-b"));

        let merged = left.merge(&right).merged;
        assert_eq!(merged.fields["author"].v, serde_json::json!("B author"));
        assert_eq!(merged.fields["title"].v, serde_json::json!("A title"));
        assert_eq!(merged.updated_at, hlc(11, 0, "dev-b"));
    }

    #[test]
    fn field_write_cannot_resurrect_a_tombstone() {
        let mut left = row("dev-a", 1);
        left.set_field("note", serde_json::json!("original"), hlc(5, 0, "dev-a"));
        left.tombstone(hlc(6, 0, "dev-a"));
        assert!(left.is_deleted());

        let mut right = row("dev-b", 1);
        right.set_field("note", serde_json::json!("late edit"), hlc(9, 0, "dev-b"));

        let merged = left.merge(&right).merged;
        assert!(
            merged.is_deleted(),
            "a later field write must not revive the row"
        );
        assert_eq!(merged.fields["note"].v, serde_json::json!("late edit"));
        assert!(merged.deleted_at.is_some());
    }

    #[test]
    fn only_a_newer_reincarnation_clears_a_tombstone() {
        let mut row_a = row("dev-a", 1);
        row_a.tombstone(hlc(10, 0, "dev-a"));

        let mut same_clock = row_a.clone();
        assert!(
            !same_clock
                .restore("revive-1", hlc(10, 0, "dev-a"))
                .expect("restore")
        );
        assert!(same_clock.is_deleted());

        let mut older = row_a.clone();
        assert!(!older.restore("revive-2", hlc(9, 0, "dev-a")).expect("restore"));
        assert!(older.is_deleted());

        let mut newer = row_a.clone();
        assert!(newer.restore("revive-3", hlc(11, 0, "dev-a")).expect("restore"));
        assert!(!newer.is_deleted());
        assert!(newer.deleted_at.is_some(), "the tombstone is kept for audit");
    }

    #[test]
    fn revival_token_is_shared_and_tombstone_never_disappears() {
        let mut deleted = row("dev-a", 1);
        deleted.tombstone(hlc(10, 0, "dev-a"));
        let mut revived = row("dev-b", 1);
        revived
            .restore("revive-1", hlc(12, 0, "dev-b"))
            .expect("restore");

        let merged = deleted.merge(&revived).merged;
        assert!(!merged.is_deleted());
        assert_eq!(merged.deleted_at, Some(hlc(10, 0, "dev-a")));
        assert_eq!(
            merged.reincarnation.as_ref().map(|r| r.token.as_str()),
            Some("revive-1")
        );

        let again = merged.merge(&deleted).merged;
        assert!(!again.is_deleted(), "merging the tombstone back must not re-delete");
    }

    #[test]
    fn updated_at_is_the_maximum_of_everything() {
        let mut left = row("dev-a", 1);
        left.set_field("a", serde_json::json!(1), hlc(4, 0, "dev-a"));
        left.tombstone(hlc(6, 0, "dev-a"));
        left.restore("revive", hlc(20, 0, "dev-a")).expect("restore");

        let mut right = row("dev-b", 3);
        right.set_field("b", serde_json::json!(2), hlc(30, 0, "dev-b"));
        right.set_field("a", serde_json::json!(3), hlc(5, 0, "dev-b"));

        let merged = left.merge(&right).merged;
        assert_eq!(merged.updated_at, hlc(30, 0, "dev-b"));
    }

    #[test]
    fn rejects_empty_reincarnation_token_and_bad_envelopes() {
        let mut replica = row("dev-a", 1);
        assert!(matches!(
            replica.restore("", hlc(2, 0, "dev-a")).unwrap_err(),
            SyncError::InvalidOp(_)
        ));

        replica.set_field("title", serde_json::json!("t"), hlc(2, 0, "dev-a"));
        replica
            .fields
            .get_mut("title")
            .expect("field")
            .s = device("dev-b");
        assert!(matches!(
            replica.validate_envelopes().unwrap_err(),
            SyncError::EnvelopeMismatch { .. }
        ));
    }

    fn lcg_next(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }

    fn random_replica(seed: &mut u64, dev: &str) -> Replica {
        let device = device(dev);
        let mut replica = Replica::new(Hlc::new(lcg_next(seed) % 3, 0, device.clone()));
        let field_count = (lcg_next(seed) % 4) as usize;
        for index in 0..field_count {
            let value = serde_json::json!((lcg_next(seed) % 4) as i64);
            let physical = lcg_next(seed) % 3;
            let counter = (lcg_next(seed) % 3) as u32;
            replica.set_field(
                &format!("f{index}"),
                value,
                Hlc::new(physical, counter, device.clone()),
            );
        }
        if lcg_next(seed) % 3 == 0 {
            replica.tombstone(Hlc::new(lcg_next(seed) % 3, 0, device.clone()));
        }
        if lcg_next(seed) % 4 == 0 {
            let token = format!("revive-{}", lcg_next(seed) % 3);
            replica
                .restore(&token, Hlc::new(lcg_next(seed) % 4, 0, device.clone()))
                .expect("restore");
        }
        replica
    }

    #[test]
    fn merge_laws_hold_over_random_replicas() {
        let mut seed = 0xC0FFEE_u64;
        for round in 0..200 {
            let a = random_replica(&mut seed, "dev-a");
            let b = random_replica(&mut seed, "dev-b");
            let c = random_replica(&mut seed, "dev-c");
            assert_eq!(
                a.merge(&b).merged,
                b.merge(&a).merged,
                "commutativity failed in round {round}"
            );
            assert_eq!(a.merge(&a).merged, a, "idempotence failed in round {round}");
            assert_eq!(
                a.merge(&b).merged.merge(&c).merged,
                a.merge(&b.merge(&c).merged).merged,
                "associativity failed in round {round}"
            );
        }
    }
}
