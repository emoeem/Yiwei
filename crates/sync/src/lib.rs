//! Sync protocol primitives (design report §16 and §21, ADR-0004).
//!
//! The local database is the source of truth; the server only relays and
//! merges. Client and server must therefore share *one* implementation of the
//! merge semantics — this crate is that implementation, and every rule below is
//! written so that merging is deterministic, commutative, associative and
//! idempotent, which the tests verify over randomly generated replicas.
//!
//! Implemented here:
//!
//! * [`hlc`] — hybrid logical clocks, monotonic under clock skew and rollback,
//! * [`envelope`] — per-field `{v,t,s}` envelopes and their ordering,
//! * [`replica`] — row merge with tombstones and explicit reincarnation,
//! * [`merge`] — note (three-way text) and reading-progress merge rules,
//! * [`op`] — operations, idempotent ingestion and the push/pull shapes,
//! * [`transport`] — protocol-level HTTP shapes, the [`Transport`] trait and a
//!   [`SyncClient`] helper. No concrete HTTP client is bundled; the app layer
//!   injects one (reqwest, fetch, …).
//!
//! Not implemented here (and deliberately not faked): the server-side PostgreSQL
//! merge functions and the blob protocol. Those belong to MVP 3 and the
//! `server/` tree.

pub mod envelope;
pub mod error;
pub mod hlc;
pub mod merge;
pub mod op;
pub mod replica;
pub mod transport;

pub use envelope::FieldEnvelope;
pub use error::SyncError;
pub use hlc::{DeviceId, Hlc, HlcClock, HlcError};
pub use merge::{
    ChapterProgress, MergeSource, NoteMerge, ProgressField, ProgressMerge, ProgressState,
    ProgressWarning, ThreeWayMerge, merge_chapter_progress, merge_note, merge_progress,
    three_way_merge,
};
pub use op::{
    ApplyOutcome, EntityKind, Op, OpKind, OpLog, PullRequest, PullResponse, PushOutcome, StoredOp,
    payload_hash,
};
pub use replica::{Fields, MergeResult, MergeStats, Reincarnation, Replica};
pub use transport::{
    PullRequestHttp, PullResponseHttp, PushRequest, PushResponse, RemoteApplyOutcome,
    SyncClient, Transport, TransportError, batch_idempotency_key,
};

#[cfg(feature = "reqwest-transport")]
pub use transport::ReqwestTransport;
