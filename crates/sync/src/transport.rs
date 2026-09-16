//! HTTP transport abstractions for the sync protocol (§16.5, §21.6).
//!
//! This module deliberately carries **no HTTP client**. It only defines:
//!
//! * [`Transport`] — the minimal trait a concrete transport must satisfy.
//! * [`PushRequest`] / [`PushResponse`] / [`PullRequestHttp`] / [`PullResponseHttp`] —
//!   the JSON shapes that go over the wire.
//! * [`SyncClient`] — a high-level helper that drives a local [`OpLog`] through a
//!   transport, with idempotency keys and cursor pagination.
//!
//! Real implementations live where HTTP is cheap: `apps/desktop` (reqwest),
//! the WASM front-end (fetch), or tests (in-memory).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::SyncError;
use crate::hlc::DeviceId;
use crate::op::{ApplyOutcome, Op, OpLog, PullRequest, PullResponse, PushOutcome, StoredOp};

/// SHA-256 hex digest of a batch of op IDs, used as idempotency key.
use reader_model::sha256_hex;

/// Per-Op outcome returned by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RemoteApplyOutcome {
    Applied,
    Duplicate,
    #[serde(rename_all = "camelCase")]
    Rejected { reason: String },
}

impl From<RemoteApplyOutcome> for ApplyOutcome {
    fn from(value: RemoteApplyOutcome) -> Self {
        match value {
            RemoteApplyOutcome::Applied => ApplyOutcome::Applied,
            RemoteApplyOutcome::Duplicate => ApplyOutcome::Duplicate,
            RemoteApplyOutcome::Rejected { reason } => {
                ApplyOutcome::Rejected(SyncError::InvalidOp(reason))
            }
        }
    }
}

/// A push batch as sent over HTTP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    /// The device that is pushing these ops.
    pub device_id: DeviceId,
    /// The operations, in local sequence order.
    pub ops: Vec<Op>,
    /// Idempotency key — if the server has already seen this batch it returns the
    /// cached result without re-applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Server response to a push batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    /// Per-op outcome keyed by `op_id`.
    pub results: HashMap<String, RemoteApplyOutcome>,
}

/// HTTP-level pull request (same payload as [`PullRequest`]; declared here so
/// the transport module is self-contained).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHttp {
    pub device_id: DeviceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub limit: u32,
}

impl From<PullRequest> for PullRequestHttp {
    fn from(value: PullRequest) -> Self {
        Self {
            device_id: value.device_id,
            cursor: value.cursor,
            limit: value.limit,
        }
    }
}

/// HTTP-level pull response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponseHttp {
    pub ops: Vec<StoredOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl From<PullResponseHttp> for PullResponse {
    fn from(value: PullResponseHttp) -> Self {
        Self {
            ops: value.ops,
            next_cursor: value.next_cursor,
            has_more: value.has_more,
        }
    }
}

/// Errors raised by a transport.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    /// Network or I/O failure at the transport layer.
    Network(String),
    /// Server returned an unexpected status code.
    HttpStatus { code: u16, message: String },
    /// Payload could not be decoded as the expected JSON shape.
    Decode(String),
    /// The server rejected the request with a sync-level error (auth, schema
    /// version …). `message` is the server-supplied explanation.
    Protocol { code: String, message: String },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Network(msg) => write!(f, "transport network error: {msg}"),
            TransportError::HttpStatus { code, message } => {
                write!(f, "HTTP {code}: {message}")
            }
            TransportError::Decode(msg) => write!(f, "transport decode error: {msg}"),
            TransportError::Protocol { code, message } => {
                write!(f, "sync protocol error {code}: {message}")
            }
        }
    }
}

impl std::error::Error for TransportError {}

/// Minimal trait a concrete sync transport must satisfy.
///
/// Implementations live in the app layer (desktop shell, WASM front-end). The
/// transport is *not* required to be `Send` or `Sync` — most HTTP clients have
/// their own thread rules, and the caller is responsible for scheduling.
pub trait Transport {
    /// Push a batch of operations; returns the server-side outcome map.
    fn push(&self, request: &PushRequest) -> Result<PushResponse, TransportError>;

    /// Pull operations after `cursor`; returns the next batch and pagination info.
    fn pull(&self, request: &PullRequestHttp) -> Result<PullResponseHttp, TransportError>;
}

/// High-level helper that drives an [`OpLog`] through a [`Transport`].
///
/// `SyncClient` owns nothing — every method takes `OpLog` by mutable reference
/// so the caller controls where the authoritative state lives (in-memory,
/// `reader-storage`, …).
pub struct SyncClient<'a, T: Transport + ?Sized> {
    transport: &'a T,
    device_id: DeviceId,
    pull_limit: u32,
}

impl<'a, T: Transport + ?Sized> std::fmt::Debug for SyncClient<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncClient")
            .field("device_id", &self.device_id)
            .field("pull_limit", &self.pull_limit)
            .finish_non_exhaustive()
    }
}

impl<'a, T: Transport + ?Sized> SyncClient<'a, T> {
    pub fn new(transport: &'a T, device_id: DeviceId) -> Self {
        Self {
            transport,
            device_id,
            pull_limit: 500,
        }
    }

    /// Override the max number of ops fetched per pull (default 500).
    pub fn with_pull_limit(mut self, limit: u32) -> Self {
        self.pull_limit = limit.max(1);
        self
    }

    /// Push a batch of ops. Returns the local [`PushOutcome`] (what the server
    /// accepted / rejected / duplicated), ready to be merged back into the
    /// caller's local state.
    pub fn push_ops(&self, ops: &[Op]) -> Result<PushOutcome, TransportError> {
        let request = PushRequest {
            device_id: self.device_id.clone(),
            ops: ops.to_vec(),
            idempotency_key: Some(batch_idempotency_key(ops)),
        };
        let response = self.transport.push(&request)?;

        let mut outcome = PushOutcome::default();
        for op in ops {
            let remote = response
                .results
                .get(&op.op_id)
                .cloned()
                .unwrap_or(RemoteApplyOutcome::Rejected {
                    reason: "server omitted result".to_string(),
                });
            let local: ApplyOutcome = remote.into();
            match &local {
                ApplyOutcome::Applied => outcome.applied += 1,
                ApplyOutcome::Duplicate => outcome.duplicates += 1,
                ApplyOutcome::Rejected(_) => outcome.rejected += 1,
            }
            outcome.results.push((op.op_id.clone(), local));
        }
        Ok(outcome)
    }

    /// Pull **all** outstanding ops from the server, applying each page to
    /// `log`. Returns the total number of ops applied.
    pub fn pull_all(&self, log: &mut OpLog) -> Result<usize, TransportError> {
        let mut cursor: Option<String> = None;
        let mut total = 0usize;
        loop {
            let request = PullRequestHttp {
                device_id: self.device_id.clone(),
                cursor: cursor.clone(),
                limit: self.pull_limit,
            };
            let response = self.transport.pull(&request)?;
            let count = response.ops.len();
            for stored in &response.ops {
                log.apply(&stored.op);
            }
            total += count;
            if !response.has_more {
                break;
            }
            cursor = response.next_cursor.clone();
            if cursor.is_none() {
                break;
            }
        }
        Ok(total)
    }
}

/// Deterministic idempotency key for a batch of ops.
///
/// Built from every `op_id` in order so replaying the exact same batch
/// (even across devices or transport retries) produces the same key.
pub fn batch_idempotency_key(ops: &[Op]) -> String {
    let mut digest = Vec::with_capacity(ops.len() * 36 + 8);
    digest.extend_from_slice(b"batch:");
    for op in ops {
        digest.extend_from_slice(op.op_id.as_bytes());
        digest.push(b':');
    }
    sha256_hex(&digest)
}

#[cfg(feature = "reqwest-transport")]
#[derive(Debug)]
pub struct ReqwestTransport {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "reqwest-transport")]
impl ReqwestTransport {
    pub fn new(base_url: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!("reader-sync/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }
}

#[cfg(feature = "reqwest-transport")]
impl Transport for ReqwestTransport {
    fn push(&self, request: &PushRequest) -> Result<PushResponse, TransportError> {
        let url = format!("{}/push", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(request)
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().unwrap_or_default();
            return Err(TransportError::HttpStatus {
                code: status.as_u16(),
                message,
            });
        }
        resp.json()
            .map_err(|e| TransportError::Decode(e.to_string()))
    }

    fn pull(&self, request: &PullRequestHttp) -> Result<PullResponseHttp, TransportError> {
        let url = format!("{}/pull", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(request)
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().unwrap_or_default();
            return Err(TransportError::HttpStatus {
                code: status.as_u16(),
                message,
            });
        }
        resp.json()
            .map_err(|e| TransportError::Decode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::DeviceId;
    use crate::op::{EntityKind, OpKind};

    struct MockTransport {
        push_response: PushResponse,
        pull_responses: Vec<PullResponseHttp>,
        pull_call_count: std::cell::RefCell<usize>,
    }

    impl Transport for MockTransport {
        fn push(&self, _request: &PushRequest) -> Result<PushResponse, TransportError> {
            Ok(self.push_response.clone())
        }
        fn pull(&self, _request: &PullRequestHttp) -> Result<PullResponseHttp, TransportError> {
            let mut count = self.pull_call_count.borrow_mut();
            let resp = self
                .pull_responses
                .get(*count)
                .cloned()
                .unwrap_or_else(|| PullResponseHttp {
                    ops: vec![],
                    next_cursor: None,
                    has_more: false,
                });
            *count += 1;
            Ok(resp)
        }
    }

    fn sample_op(op_id: &str) -> Op {
        let device = DeviceId::parse("dev-1").unwrap();
        Op {
            op_id: op_id.to_string(),
            entity: EntityKind::Publication,
            entity_id: "pub-1".to_string(),
            kind: OpKind::Upsert,
            base_revision: None,
            fields: Default::default(),
            hlc: crate::hlc::Hlc::new(0, 0, device.clone()),
            device_id: device,
            payload_hash: None,
        }
    }

    #[test]
    fn transport_round_trip_shapes() {
        let req = PullRequestHttp {
            device_id: DeviceId::parse("dev-1").unwrap(),
            cursor: Some("42".to_string()),
            limit: 200,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: PullRequestHttp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);

        let resp = PushResponse {
            results: HashMap::from([
                ("op-1".to_string(), RemoteApplyOutcome::Applied),
                (
                    "op-2".to_string(),
                    RemoteApplyOutcome::Rejected {
                        reason: "bad".to_string(),
                    },
                ),
            ]),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"kind\":\"applied\""));
        assert!(json.contains("\"kind\":\"rejected\""));
    }

    #[test]
    fn sync_client_pull_all_pages_until_no_more() {
        let t = MockTransport {
            push_response: PushResponse {
                results: HashMap::new(),
            },
            pull_responses: vec![
                PullResponseHttp {
                    ops: vec![StoredOp {
                        sequence: 1,
                        op: sample_op("op-remote-1"),
                    }],
                    next_cursor: Some("1".to_string()),
                    has_more: true,
                },
                PullResponseHttp {
                    ops: vec![StoredOp {
                        sequence: 2,
                        op: sample_op("op-remote-2"),
                    }],
                    next_cursor: None,
                    has_more: false,
                },
            ],
            pull_call_count: std::cell::RefCell::new(0),
        };
        let mut log = OpLog::new();
        let client = SyncClient::new(&t, DeviceId::parse("dev-local").unwrap());
        let applied = client.pull_all(&mut log).unwrap();
        assert_eq!(applied, 2);
        assert_eq!(log.op_count(), 2);
    }

    #[test]
    fn sync_client_push_maps_remote_outcomes() {
        let t = MockTransport {
            push_response: PushResponse {
                results: HashMap::from([
                    ("op-a".to_string(), RemoteApplyOutcome::Applied),
                    ("op-b".to_string(), RemoteApplyOutcome::Duplicate),
                ]),
            },
            pull_responses: vec![],
            pull_call_count: std::cell::RefCell::new(0),
        };
        let ops = vec![sample_op("op-a"), sample_op("op-b")];
        let client = SyncClient::new(&t, DeviceId::parse("dev-local").unwrap());
        let outcome = client.push_ops(&ops).unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(outcome.duplicates, 1);
        assert_eq!(outcome.rejected, 0);
    }

    #[test]
    fn batch_idempotency_key_is_deterministic() {
        let a = vec![sample_op("op-x"), sample_op("op-y")];
        let b = vec![sample_op("op-x"), sample_op("op-y")];
        let c = vec![sample_op("op-y"), sample_op("op-x")];
        assert_eq!(batch_idempotency_key(&a), batch_idempotency_key(&b));
        assert_ne!(batch_idempotency_key(&a), batch_idempotency_key(&c));
    }
}
