//! Cancellable JSON-over-HTTP transport shared by every provider adapter.

use super::{ApiKey, ProviderError};
use crate::message::CancelToken;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    /// Header name/value pairs. Values may contain the API key: never log them.
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait HttpTransport: Send + Sync {
    /// Perform one POST. Must return `Err(Cancelled)` promptly once `cancel` fires.
    fn post(&self, req: HttpRequest, cancel: &CancelToken) -> Result<HttpResponse, ProviderError>;
}

const POLL: Duration = Duration::from_millis(25);

/// Transport used when the `remote-http` feature is off: refuses every request with a typed
/// error and never touches the network.
pub struct OfflineTransport;

impl HttpTransport for OfflineTransport {
    fn post(&self, req: HttpRequest, _: &CancelToken) -> Result<HttpResponse, ProviderError> {
        Err(ProviderError::TransportNotCompiledIn(req.url))
    }
}

/// The transport `build_provider` uses: HTTPS when built with `remote-http`, otherwise
/// [`OfflineTransport`].
pub fn default_transport() -> Arc<dyn HttpTransport> {
    #[cfg(feature = "remote-http")]
    {
        Arc::new(UreqTransport)
    }
    #[cfg(not(feature = "remote-http"))]
    {
        Arc::new(OfflineTransport)
    }
}

#[cfg(feature = "remote-http")]
pub use remote::UreqTransport;

#[cfg(feature = "remote-http")]
mod remote {
    use super::*;
    use std::sync::mpsc;

    /// Blocking HTTPS client (rustls) run on a worker thread so cancellation returns
    /// immediately; an abandoned request is bounded by its global timeout.
    pub struct UreqTransport;

    /// Response bodies larger than this are rejected (bounded memory).
    const MAX_BODY_BYTES: u64 = 16 * 1024 * 1024;

    impl HttpTransport for UreqTransport {
        fn post(
            &self,
            req: HttpRequest,
            cancel: &CancelToken,
        ) -> Result<HttpResponse, ProviderError> {
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let (tx, rx) = mpsc::channel();
            let timeout = req.timeout;
            std::thread::spawn(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(req.timeout))
                    .http_status_as_error(false)
                    .build()
                    .into();
                let mut b = agent.post(&req.url);
                for (k, v) in &req.headers {
                    b = b.header(k.as_str(), v.as_str());
                }
                let result = match b.send(req.body.as_bytes()) {
                    Ok(mut resp) => {
                        let status = resp.status().as_u16();
                        resp.body_mut()
                            .with_config()
                            .limit(MAX_BODY_BYTES)
                            .read_to_string()
                            .map(|body| HttpResponse { status, body })
                            .map_err(map_ureq_error)
                    }
                    Err(e) => Err(map_ureq_error(e)),
                };
                let _ = tx.send(result);
            });
            let deadline = Instant::now() + timeout + Duration::from_secs(5);
            loop {
                if cancel.is_cancelled() {
                    return Err(ProviderError::Cancelled);
                }
                match rx.recv_timeout(POLL) {
                    Ok(r) => return r,
                    Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() > deadline => {
                        return Err(ProviderError::Timeout)
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(ProviderError::Transport(
                            "HTTP worker ended unexpectedly".into(),
                        ))
                    }
                }
            }
        }
    }

    fn map_ureq_error(e: ureq::Error) -> ProviderError {
        match e {
            ureq::Error::Timeout(_) => ProviderError::Timeout,
            other => ProviderError::Transport(other.to_string()),
        }
    }
}

/// Resolved connection settings for one provider instance.
pub struct Endpoint {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<ApiKey>,
    pub timeout: Duration,
    pub max_retries: u32,
    pub transport: Arc<dyn HttpTransport>,
}

fn snippet(body: &str) -> String {
    let s: String = body.chars().take(500).collect();
    s.replace(['\r', '\n'], " ")
}

/// Map a non-2xx response to a typed error.
pub(crate) fn status_error(status: u16, body: &str) -> ProviderError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str().map(String::from))
        })
        .unwrap_or_else(|| snippet(body));
    match status {
        401 | 403 => ProviderError::Auth { status, message },
        429 => ProviderError::RateLimited(message),
        500..=599 => ProviderError::Server { status, message },
        _ => ProviderError::Http { status, message },
    }
}

/// Sleep for `d` unless cancelled first.
fn cancellable_sleep(d: Duration, cancel: &CancelToken) -> Result<(), ProviderError> {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
    }
    Ok(())
}

impl Endpoint {
    /// POST `body` as JSON to `base_url + path`, with bounded retries on transient failures.
    pub fn post_json(
        &self,
        path: &str,
        headers: Vec<(String, String)>,
        body: &Value,
        cancel: &CancelToken,
    ) -> Result<Value, ProviderError> {
        let mut all = vec![("content-type".to_string(), "application/json".to_string())];
        all.extend(headers);
        let req = HttpRequest {
            url: format!("{}{}", self.base_url, path),
            headers: all,
            body: body.to_string(),
            timeout: self.timeout,
        };
        let mut attempt = 0;
        loop {
            let result = self.transport.post(req.clone(), cancel).and_then(|r| {
                if (200..300).contains(&r.status) {
                    serde_json::from_str::<Value>(&r.body)
                        .map_err(|e| ProviderError::Malformed(format!("{e}: {}", snippet(&r.body))))
                } else {
                    Err(status_error(r.status, &r.body))
                }
            });
            match result {
                Err(e) if e.is_retryable() && attempt < self.max_retries => {
                    attempt += 1;
                    // shortcut: fixed exponential backoff ignoring Retry-After; honour the
                    // header once responses expose it through the transport.
                    cancellable_sleep(Duration::from_millis(500 << attempt), cancel)?;
                }
                other => return other,
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Recording transport that replays canned responses (no network).
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct RecordingTransport {
        pub requests: Mutex<Vec<HttpRequest>>,
        pub responses: Mutex<std::collections::VecDeque<HttpResponse>>,
    }

    impl RecordingTransport {
        pub fn with(responses: Vec<(u16, &str)>) -> Arc<RecordingTransport> {
            Arc::new(RecordingTransport {
                requests: Mutex::default(),
                responses: Mutex::new(
                    responses
                        .into_iter()
                        .map(|(status, body)| HttpResponse {
                            status,
                            body: body.into(),
                        })
                        .collect(),
                ),
            })
        }
        pub fn bodies(&self) -> Vec<Value> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .map(|r| serde_json::from_str(&r.body).unwrap())
                .collect()
        }
    }

    impl HttpTransport for RecordingTransport {
        fn post(
            &self,
            req: HttpRequest,
            cancel: &CancelToken,
        ) -> Result<HttpResponse, ProviderError> {
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            self.requests.lock().unwrap().push(req);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| ProviderError::Exhausted("no canned HTTP response".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::RecordingTransport;
    use super::*;

    fn ep(t: Arc<RecordingTransport>, retries: u32) -> Endpoint {
        Endpoint {
            base_url: "http://test".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            max_retries: retries,
            transport: t,
        }
    }

    #[test]
    fn retries_transient_errors_then_succeeds() {
        let t = RecordingTransport::with(vec![(503, "busy"), (200, r#"{"ok":true}"#)]);
        let v = ep(t.clone(), 2)
            .post_json("/x", vec![], &serde_json::json!({}), &CancelToken::new())
            .unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(t.requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn does_not_retry_client_errors_and_maps_status() {
        let t = RecordingTransport::with(vec![(401, r#"{"error":{"message":"bad key"}}"#)]);
        let e = ep(t.clone(), 2)
            .post_json("/x", vec![], &serde_json::json!({}), &CancelToken::new())
            .unwrap_err();
        assert_eq!(
            e,
            ProviderError::Auth {
                status: 401,
                message: "bad key".into()
            }
        );
        assert_eq!(t.requests.lock().unwrap().len(), 1);
        assert!(matches!(
            status_error(429, "x"),
            ProviderError::RateLimited(_)
        ));
        assert!(matches!(status_error(400, "x"), ProviderError::Http { .. }));
    }

    #[test]
    fn cancelled_before_send_never_reaches_transport() {
        let t = RecordingTransport::with(vec![(200, "{}")]);
        let c = CancelToken::new();
        c.cancel();
        let e = ep(t.clone(), 0)
            .post_json("/x", vec![], &serde_json::json!({}), &c)
            .unwrap_err();
        assert_eq!(e, ProviderError::Cancelled);
        assert!(t.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn offline_transport_refuses_with_a_typed_error() {
        let r = OfflineTransport.post(
            HttpRequest {
                url: "https://api.anthropic.com/v1/messages".into(),
                headers: vec![],
                body: String::new(),
                timeout: Duration::from_secs(1),
            },
            &CancelToken::new(),
        );
        assert_eq!(
            r.unwrap_err(),
            ProviderError::TransportNotCompiledIn("https://api.anthropic.com/v1/messages".into())
        );
        // Not a transient error: the endpoint never retries it.
        assert!(!ProviderError::TransportNotCompiledIn(String::new()).is_retryable());
    }

    #[cfg(not(feature = "remote-http"))]
    #[test]
    fn without_remote_http_every_remote_provider_reports_missing_transport() {
        use crate::message::{ChatMessage, ChatRequest};
        use crate::provider::{build_provider, ApiKey, ProviderConfig, ProviderKind};
        for kind in [
            ProviderKind::OpenAiCompatible,
            ProviderKind::Anthropic,
            ProviderKind::Ollama,
        ] {
            let mut cfg = ProviderConfig::new(kind);
            cfg.model = Some("m".into());
            cfg.api_key = Some(ApiKey::new("k"));
            let p = build_provider(&cfg).unwrap();
            let req = ChatRequest {
                system: "s".into(),
                messages: vec![ChatMessage::User { text: "u".into() }],
                tools: vec![],
                max_tokens: 10,
            };
            let e = p.complete(&req, &CancelToken::new()).unwrap_err();
            assert!(
                matches!(e, ProviderError::TransportNotCompiledIn(_)),
                "{kind:?}: {e}"
            );
        }
    }

    #[cfg(feature = "remote-http")]
    #[test]
    fn ureq_transport_refuses_a_cancelled_token_without_network() {
        let c = CancelToken::new();
        c.cancel();
        let r = UreqTransport.post(
            HttpRequest {
                url: "http://127.0.0.1:9/".into(),
                headers: vec![],
                body: String::new(),
                timeout: Duration::from_secs(1),
            },
            &c,
        );
        assert_eq!(r.unwrap_err(), ProviderError::Cancelled);
    }

    #[test]
    fn malformed_success_body_is_reported() {
        let t = RecordingTransport::with(vec![(200, "not json")]);
        let e = ep(t, 0)
            .post_json("/x", vec![], &serde_json::json!({}), &CancelToken::new())
            .unwrap_err();
        assert!(matches!(e, ProviderError::Malformed(_)));
    }
}
