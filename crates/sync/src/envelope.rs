//! Per-field CRDT envelopes (design report §16.3).
//!
//! Every synchronised field is stored as `{ v, t, s }`: the value, the hybrid
//! logical clock that wrote it, and the device that wrote it. Merging is then a
//! per-field "greater clock wins" operation, which is why a whole row is never
//! overwritten by a stale device.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::error::SyncError;
use crate::hlc::{DeviceId, Hlc};

/// A single synchronised field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldEnvelope<T> {
    /// The value.
    pub v: T,
    /// Clock of the write.
    pub t: Hlc,
    /// Device that performed the write.
    pub s: DeviceId,
}

impl<T> FieldEnvelope<T> {
    /// Build an envelope from a value and the clock that produced it.
    pub fn new(v: T, t: Hlc) -> Self {
        let s = t.device().clone();
        Self { v, t, s }
    }

    /// Reject envelopes whose redundant device field contradicts the clock.
    pub fn validate(&self, field: &str) -> Result<(), SyncError> {
        if &self.s != self.t.device() {
            return Err(SyncError::EnvelopeMismatch {
                field: field.to_string(),
                declared: self.s.to_string(),
                clock: self.t.device().to_string(),
            });
        }
        Ok(())
    }
}

impl<T: PartialEq> FieldEnvelope<T> {
    /// Whether this envelope should replace `other`, judged by clock only.
    ///
    /// Equal clocks are treated as "no newer value": a correct client can never
    /// produce two different values for the same clock. Untrusted input is
    /// handled by [`FieldEnvelope::cmp_total`], which also orders the values so
    /// that both sides of a merge pick the same winner regardless of argument
    /// order.
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.cmp_envelope(other) == Ordering::Greater
    }

    /// Keep the envelope with the greater clock.
    pub fn pick_newer(self, other: Self) -> Self {
        if self.is_newer_than(&other) { self } else { other }
    }

    fn cmp_envelope(&self, other: &Self) -> Ordering {
        self.t.cmp(&other.t).then_with(|| self.s.cmp(&other.s))
    }
}

impl FieldEnvelope<serde_json::Value> {
    /// Total order over clock, device and value.
    ///
    /// Used by the replica merge so that no input — not even a hostile client
    /// sending two different values under one clock — can make the merge
    /// non-commutative.
    pub fn cmp_total(&self, other: &Self) -> Ordering {
        self.cmp_envelope(other).then_with(|| {
            let left = serde_json::to_string(&self.v).unwrap_or_default();
            let right = serde_json::to_string(&other.v).unwrap_or_default();
            left.cmp(&right)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("device")
    }

    fn envelope(value: &str, physical_ms: u64, counter: u32, dev: &str) -> FieldEnvelope<String> {
        FieldEnvelope::new(
            value.to_string(),
            Hlc::new(physical_ms, counter, device(dev)),
        )
    }

    #[test]
    fn newer_clock_wins() {
        let older = envelope("old", 1, 0, "dev-a");
        let newer = envelope("new", 2, 0, "dev-b");
        assert!(newer.is_newer_than(&older));
        assert!(!older.is_newer_than(&newer));
        assert_eq!(older.clone().pick_newer(newer.clone()), newer);
        assert_eq!(newer.pick_newer(older), envelope("new", 2, 0, "dev-b"));
    }

    #[test]
    fn equal_clocks_still_yield_a_total_order() {
        let left = FieldEnvelope::new(serde_json::json!(1), Hlc::new(5, 0, device("dev-a")));
        let right = FieldEnvelope::new(serde_json::json!(2), Hlc::new(5, 0, device("dev-a")));
        assert_eq!(
            left.cmp_total(&right),
            right.cmp_total(&left).reverse(),
            "value tie-break must be antisymmetric"
        );
        assert_ne!(left.cmp_total(&right), Ordering::Equal);
    }

    #[test]
    fn validate_detects_contradicting_device() {
        let mut env = envelope("v", 1, 0, "dev-a");
        assert!(env.validate("title").is_ok());
        env.s = device("dev-b");
        assert!(matches!(
            env.validate("title").unwrap_err(),
            SyncError::EnvelopeMismatch { field, .. } if field == "title"
        ));
    }
}
