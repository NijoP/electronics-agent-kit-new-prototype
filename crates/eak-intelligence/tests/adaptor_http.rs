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
    let (base, seen) = serve(|m, p| match (m, p) {
        ("GET", "/v1/models") => (200, r#"{"data":[{"id":"zeta"},{"id":"alpha"}]}"#.into()),
        ("POST", "/v1/chat/completions") => (
            200,
            r#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}"#.into(),
        ),
        _ => (404, r#"{"error":{"message":"no route"}}"#.into()),
    });
    let c = cfg(ProviderKind::OpenAiCompatible, &format!("{base}/v1"), Some("sk-local"));
    let ids = list_models(&c, default_transport(), &CancelToken::new()).unwrap();
    assert_eq!(ids, ["alpha", "zeta"]);
    let r = test_connection(&c, default_transport(), &CancelToken::new());
    assert!(r.ok, "{r:?}");
    assert!(r.message.contains("OK"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0]
        .headers
        .contains(&("authorization".into(), "Bearer sk-local".into())));
    let body: serde_json::Value = serde_json::from_str(&seen[1].body).unwrap();
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["max_tokens"], 16);
}

#[test]
fn anthropic_and_ollama_shapes_over_real_http() {
    let (base, seen) = serve(|m, p| match (m, p) {
        ("GET", p) if p.starts_with("/v1/models") => (200, r#"{"data":[{"id":"claude-test"}]}"#.into()),
        ("POST", "/v1/messages") => (
            200,
            r#"{"content":[{"type":"text","text":"OK"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":1}}"#.into(),
        ),
        ("GET", "/api/tags") => (200, r#"{"models":[{"name":"llama3:8b"}]}"#.into()),
        ("POST", "/api/chat") => (
            200,
            r#"{"message":{"role":"assistant","content":"OK"},"done":true}"#.into(),
        ),
        _ => (404, "{}".into()),
    });
    let a = cfg(ProviderKind::Anthropic, &base, Some("ak-local"));
    assert_eq!(
        list_models(&a, default_transport(), &CancelToken::new()).unwrap(),
        ["claude-test"]
    );
    assert!(test_connection(&a, default_transport(), &CancelToken::new()).ok);
    let o = cfg(ProviderKind::Ollama, &base, None);
    assert_eq!(
        list_models(&o, default_transport(), &CancelToken::new()).unwrap(),
        ["llama3:8b"]
    );
    assert!(test_connection(&o, default_transport(), &CancelToken::new()).ok);
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
    assert!(!r.message.contains("sk-bad-key"), "{}", r.message);
    let e = list_models(&c, default_transport(), &CancelToken::new()).unwrap_err();
    assert!(!e.to_string().contains("sk-bad-key"), "{e}");
}

#[test]
fn an_unreachable_endpoint_reports_a_transport_error() {
    // Port 9 (discard) on loopback refuses the connection.
    let c = cfg(ProviderKind::OpenAiCompatible, "http://127.0.0.1:9/v1", None);
    let r = test_connection(&c, default_transport(), &CancelToken::new());
    assert!(!r.ok, "{r:?}");
}
