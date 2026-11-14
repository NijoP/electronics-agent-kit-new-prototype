//! The adaptor against a real local HTTP server over the real `ureq` transport: no mocks and no
//! network beyond loopback. Needs the `remote-http` feature.
#![cfg(feature = "remote-http")]

use eak_intelligence::message::CancelToken;
use eak_intelligence::provider::adaptor::{list_models, test_connection};
use eak_intelligence::provider::http::default_transport;
use eak_intelligence::provider::{ApiKey, ProviderConfig, ProviderKind};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// One-thread HTTP/1.1 server: `route(method, path)` -> (status, json body).
fn serve(
    route: impl Fn(&str, &str) -> (u16, String) + Send + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let mut headers = Vec::new();
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end().to_string();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
                    if k == "content-length" {
                        len = v.parse().unwrap_or(0);
                    }
                    headers.push((k, v));
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let (status, out) = route(&method, &path);
            log.lock().unwrap().push(Seen {
                method,
                path,
                headers,
                body: String::from_utf8_lossy(&body).into_owned(),
            });
            let resp = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                out.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (base, seen)
}

fn cfg(kind: ProviderKind, base: &str, key: Option<&str>) -> ProviderConfig {
    let mut c = ProviderConfig::new(kind);
    c.base_url = Some(base.to_string());
    c.model = Some("test-model".into());
    c.api_key = key.map(ApiKey::new);
    c.timeout_secs = 10;
    c
}

#[test]
fn openai_compatible_list_and_test_over_real_http() {
    let (base, seen) = serve(|m, p| {
        match (m, p) {
        ("GET", "/v1/models") => (200, r#"{"data":[{"id":"zeta"},{"id":"test-model"}]}"#.into()),
        ("POST", "/v1/chat/completions") => (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"O\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"K\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into(),
        ),
        _ => (404, r#"{"error":{"message":"no route"}}"#.into()),
    }
    });
    let c = cfg(
        ProviderKind::OpenAiCompatible,
        &format!("{base}/v1"),
        Some("sk-local"),
    );
    let ids = ids(list_models(&c, default_transport(), &CancelToken::new()).unwrap());
    assert_eq!(ids, ["test-model", "zeta"]);
    let r = test_connection(&c, default_transport(), &CancelToken::new());
    assert!(r.ok, "{r:?}");
    assert!(r.message.contains("streamed 2 chunk(s)"), "{}", r.message);
    assert!(r.message.contains("OK"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0]
        .headers
        .contains(&("authorization".into(), "Bearer sk-local".into())));
    let body: serde_json::Value = serde_json::from_str(&seen[2].body).unwrap();
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["stream"], true);
}

fn ids(models: Vec<eak_intelligence::provider::catalog::ModelInfo>) -> Vec<String> {
    models.into_iter().map(|m| m.id).collect()
}

/// A server that writes an SSE reply in two chunks and holds the second back until the
/// client has seen the first delta: proves the real transport streams incrementally.
#[test]
fn streaming_is_incremental_over_a_real_socket() {
    use eak_intelligence::{ChatMessage, ChatRequest, StreamEvent};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut len = 0usize;
        loop {
            let mut h = String::new();
            r.read_line(&mut h).unwrap();
            if h.trim().is_empty() {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; len];
        r.read_exact(&mut body).unwrap();
        let chunk = |s: &mut std::net::TcpStream, data: &str| {
            write!(s, "{:x}\r\n{data}\r\n", data.len()).unwrap();
            s.flush().unwrap();
        };
        write!(s, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n").unwrap();
        chunk(
            &mut s,
            "data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n",
        );
        gate_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("first delta never reached the client");
        chunk(&mut s, "data: {\"choices\":[{\"delta\":{\"content\":\" second\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
        write!(s, "0\r\n\r\n").unwrap();
    });
    let c = cfg(ProviderKind::OpenAiCompatible, &format!("{base}/v1"), None);
    let p = eak_intelligence::provider::build_provider_with(&c, default_transport()).unwrap();
    let mut events = vec![];
    let r = p
        .stream(
            &ChatRequest {
                system: "s".into(),
                messages: vec![ChatMessage::User { text: "u".into() }],
                tools: vec![],
                max_tokens: 8,
            },
            &CancelToken::new(),
            &mut |e| {
                if events.is_empty() {
                    let _ = gate_tx.send(());
                }
                events.push(e);
            },
        )
        .unwrap();
    assert_eq!(r.text, "first second");
    assert_eq!(
        events[0],
        StreamEvent::TextDelta {
            text: "first".into()
        }
    );
}

#[test]
fn anthropic_and_ollama_shapes_over_real_http() {
    let (base, seen) = serve(|m, p| {
        match (m, p) {
        ("GET", p) if p.starts_with("/v1/models") => (200, r#"{"data":[{"id":"test-model","display_name":"Test"}]}"#.into()),
        ("POST", "/v1/messages") => (
            200,
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3}}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"OK\"}}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n".into(),
        ),
        ("GET", "/api/tags") => (200, r#"{"models":[{"name":"test-model:latest"}]}"#.into()),
        ("POST", "/api/chat") => (
            200,
            "{\"message\":{\"role\":\"assistant\",\"content\":\"OK\"},\"done\":false}\n{\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\"}\n".into(),
        ),
        _ => (404, "{}".into()),
    }
    });
    let a = cfg(ProviderKind::Anthropic, &base, Some("ak-local"));
    let listed = list_models(&a, default_transport(), &CancelToken::new()).unwrap();
    assert_eq!(listed[0].id, "test-model");
    assert_eq!(listed[0].name.as_deref(), Some("Test"));
    let r = test_connection(&a, default_transport(), &CancelToken::new());
    assert!(r.ok, "{r:?}");
    let o = cfg(ProviderKind::Ollama, &base, None);
    assert_eq!(
        ids(list_models(&o, default_transport(), &CancelToken::new()).unwrap()),
        ["test-model:latest"]
    );
    let r = test_connection(&o, default_transport(), &CancelToken::new());
    assert!(r.ok, "{r:?}");
    let seen = seen.lock().unwrap();
    assert!(seen[0]
        .headers
        .contains(&("x-api-key".into(), "ak-local".into())));
    assert!(seen.iter().any(|s| s.path == "/api/chat"));
}

#[test]
fn provider_error_text_is_reported_and_the_key_is_not() {
    let (base, _) = serve(|_, _| {
        (
            401,
            r#"{"error":{"message":"Invalid key sk-bad-key for this org"}}"#.into(),
        )
    });
    let c = cfg(ProviderKind::OpenAiCompatible, &base, Some("sk-bad-key"));
    let r = test_connection(&c, default_transport(), &CancelToken::new());
    assert!(!r.ok);
    assert!(r.message.contains("Invalid key"), "{}", r.message);
    assert!(r.message.contains("401"), "{}", r.message);
    assert_eq!(
        r.error_class,
        Some(eak_intelligence::ErrorClass::InvalidApiKey)
    );
    assert!(!r.message.contains("sk-bad-key"), "{}", r.message);
    let e = list_models(&c, default_transport(), &CancelToken::new()).unwrap_err();
    assert!(!e.to_string().contains("sk-bad-key"), "{e}");
}

#[test]
fn an_unreachable_endpoint_reports_a_transport_error() {
    // Port 9 (discard) on loopback refuses the connection.
    let c = cfg(
        ProviderKind::OpenAiCompatible,
        "http://127.0.0.1:9/v1",
        None,
    );
    let r = test_connection(&c, default_transport(), &CancelToken::new());
    assert!(!r.ok, "{r:?}");
    assert_eq!(
        r.error_class,
        Some(eak_intelligence::ErrorClass::NetworkError),
        "{r:?}"
    );
    assert!(r.message.starts_with("Network error"), "{}", r.message);
}
