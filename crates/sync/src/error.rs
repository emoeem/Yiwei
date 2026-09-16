//! Errors shared by the sync primitives.

use crate::hlc::HlcError;

/// Failure modes when validating or applying sync data.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SyncError {
    /// A clock value was invalid.
    #[error(transparent)]
    Clock(#[from] HlcError),
    /// A field envelope's `s` did not match its clock's device.
    #[error("field {field:?} declares device {declared} but its clock was written by {clock}")]
    EnvelopeMismatch {
        /// Field name.
        field: String,
        /// Device in the envelope's `s`.
        declared: String,
        /// Device inside the clock value.
        clock: String,
    },
    /// A field envelope's clock was written by another device than the op.
    #[error("operation by {op_device} carries a field stamped by {field_device}")]
    OpDeviceMismatch {
        /// Device that submitted the operation.
        op_device: String,
        /// Device recorded in the field envelope.
        field_device: String,
    },
    /// The payload hash did not match the payload.
    #[error("payload hash mismatch: declared {declared}, computed {computed}")]
    PayloadHashMismatch {
        /// Hash declared by the sender.
        declared: String,
        /// Hash computed locally.
        computed: String,
    },
    /// The operation itself was malformed.
    #[error("invalid operation: {0}")]
    InvalidOp(String),
}

