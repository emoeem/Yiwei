#![cfg(feature = "reqwest-transport")]

//! Integration tests that drive [`ReqwestTransport`] against a real HTTP listener.
//!
//! The listener is just enough of HTTP/1.1 to accept `POST /push` and `POST /pull`
//! — it parses `Content-Length`, reads the JSON body, and writes a response
//! back. This is deliberately not a web server and deliberately no new crates:
//! `std::net` is enough to verify that the transport serialises correctly,
//! preserves idempotency keys, propagates HTTP status codes, and surfaces
//! decode failures.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use reader_sync::hlc::{DeviceId, Hlc, HlcClock};
use reader_sync::op::{EntityKind, Op, OpKind, OpLog, StoredOp};
use reader_sync::transport::{
    PullRequestHttp, PullResponseHttp, PushRequest, PushResponse, RemoteApplyOutcome,
    ReqwestTransport, Transport, TransportError, batch_idempotency_key,
};

fn sample_op(op_id: &str, device: &DeviceId) -> Op {
    Op {
        op_id: op_id.to_string(),
        entity: EntityKind::Publication,
        entity_id: "pub-1".to_string(),
        kind: OpKind::Upsert,
        base_revision: None,
        fields: Default::default(),
        hlc: HlcClock::new(device.clone())
            .tick(1_700_000_000_000)
            .unwrap_or_else(|_| Hlc::new(0, 0, device.clone())),
        device_id: device.clone(),
        payload_hash: None,
    }
}

struct CapturedRequest {
    path: String,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn parse_http_request(stream: &mut TcpStream) -> CapturedRequest {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut buf = [0u8; 4096];
    let mut header_bytes: Vec<u8> = Vec::new();
    loop {
        let n = stream.read(&mut buf).expect("read");
        if n == 0 {
            break;
        }
        header_bytes.extend_from_slice(&buf[..n]);
        if header_bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let header_str = String::from_utf8_lossy(&header_bytes);
    let mut lines = header_str.split("\r\n");
    let first = lines.next().expect("request line");
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();

    let mut content_type = None;
    let mut content_length: usize = 0;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.to_ascii_lowercase().as_str() {
                "content-type" => content_type = Some(v.trim().to_string()),
                "content-length" => content_length = v.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    let header_end = header_bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(header_bytes.len());

    let mut body = header_bytes[header_end..].to_vec();
    while body.len() < content_length {
        let mut more = [0u8; 4096];
        match stream.read(&mut more) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&more[..n]),
            Err(_) => break,
        }
    }
    body.truncate(content_length);

    CapturedRequest {
        path,
        content_type,
        body,
    }
}

fn write_response(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) {
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).expect("write headers");
    stream.write_all(body).expect("write body");
    stream.flush().expect("flush");
}

fn spawn_listener(
    handler: impl Fn(TcpStream) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local addr").to_string();
    let handle = thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handler(stream);
        }
    });
    (format!("http://{addr}"), handle)
}

#[test]
fn push_round_trips_real_http() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<CapturedRequest>));
    let captured_clone = captured.clone();

    let responder = serde_json::to_vec(&PushResponse {
        results: [("op-1".to_string(), RemoteApplyOutcome::Applied)].into(),
    })
    .unwrap();

    let (base_url, _handle) = spawn_listener(move |mut stream| {
        let req = parse_http_request(&mut stream);
        *captured_clone.lock().unwrap() = Some(req);
        write_response(&mut stream, 200, "OK", &responder);
    });

    let device = DeviceId::parse("dev-local").unwrap();
    let op = sample_op("op-1", &device);
    let key = batch_idempotency_key(std::slice::from_ref(&op));

    let transport = ReqwestTransport::new(&base_url).expect("transport");
    let response = transport
        .push(&PushRequest {
            device_id: device.clone(),
            ops: vec![op],
            idempotency_key: Some(key.clone()),
        })
        .expect("push");

    assert_eq!(response.results.get("op-1"), Some(&RemoteApplyOutcome::Applied));

    let req = captured.lock().unwrap().take().expect("captured");
    assert_eq!(req.path, "/push");
    assert_eq!(req.content_type.as_deref(), Some("application/json"));

    let parsed: serde_json::Value =
        serde_json::from_slice(&req.body).expect("request JSON");
    assert_eq!(
        parsed.get("idempotencyKey").unwrap().as_str(),
        Some(key.as_str())
    );
    assert_eq!(
        parsed.get("deviceId").unwrap().as_str(),
        Some("dev-local")
    );
    assert_eq!(parsed.get("ops").unwrap().as_array().unwrap().len(), 1);
}

#[test]
fn pull_paginates_with_real_http() {
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls_clone = calls.clone();

    let page1 = serde_json::to_vec(&PullResponseHttp {
        ops: vec![StoredOp {
            sequence: 1,
            op: sample_op("remote-1", &DeviceId::parse("dev-remote").unwrap()),
        }],
        next_cursor: Some("1".to_string()),
        has_more: true,
    })
    .unwrap();

    let (base_url, _handle) = spawn_listener(move |mut stream| {
        let req = parse_http_request(&mut stream);
        let body_json: serde_json::Value =
            serde_json::from_slice(&req.body).expect("parse");
        calls_clone.lock().unwrap().push(body_json);
        write_response(&mut stream, 200, "OK", &page1);
    });

    let transport = ReqwestTransport::new(&base_url).expect("transport");
    let mut log = OpLog::new();

    // 第一次 pull：有 has_more=true，但测试到此为止
    // 多轮循环逻辑已经在 SyncClient 单元测试中覆盖
    let resp = transport
        .pull(&PullRequestHttp {
            device_id: DeviceId::parse("dev-local").unwrap(),
            cursor: None,
            limit: 500,
        })
        .expect("pull");

    assert!(resp.has_more);
    assert_eq!(resp.next_cursor.as_deref(), Some("1"));
    assert_eq!(resp.ops.len(), 1);
    log.apply(&resp.ops[0].op);
    assert_eq!(log.op_count(), 1);

    let all_calls = calls.lock().unwrap();
    assert_eq!(all_calls.len(), 1);
    assert!(all_calls[0].get("cursor").is_none() || all_calls[0]["cursor"].is_null());
}

#[test]
fn http_status_propagates_to_transport_error() {
    let (base_url, _handle) = spawn_listener(|mut stream| {
        let _ = parse_http_request(&mut stream);
        write_response(
            &mut stream,
            400,
            "Bad Request",
            b"{\"error\": \"missing deviceId\"}",
        );
    });

    let transport = ReqwestTransport::new(&base_url).expect("transport");
    let device = DeviceId::parse("dev-x").unwrap();
    let err = transport
        .pull(&PullRequestHttp {
            device_id: device,
            cursor: None,
            limit: 500,
        })
        .expect_err("should fail");

    match err {
        TransportError::HttpStatus { code, message } => {
            assert_eq!(code, 400);
            assert!(message.contains("missing deviceId"));
        }
        other => panic!("expected HttpStatus, got {other:?}"),
    }
}

#[test]
fn transport_decode_error_on_bad_json() {
    let (base_url, _handle) = spawn_listener(|mut stream| {
        let _ = parse_http_request(&mut stream);
        write_response(&mut stream, 200, "OK", b"not json at all");
    });

    let transport = ReqwestTransport::new(&base_url).expect("transport");
    let device = DeviceId::parse("dev-x").unwrap();
    let err = transport
        .pull(&PullRequestHttp {
            device_id: device,
            cursor: None,
            limit: 500,
        })
        .expect_err("should fail");

    assert!(matches!(err, TransportError::Decode(_)));
}

#[test]
fn push_network_error_when_unreachable() {
    let transport =
        ReqwestTransport::new("http://127.0.0.1:1").expect("transport");
    let device = DeviceId::parse("dev-x").unwrap();
    let err = transport
        .push(&PushRequest {
            device_id: device,
            ops: vec![],
            idempotency_key: None,
        })
        .expect_err("should fail");

    assert!(matches!(err, TransportError::Network(_)));
}
