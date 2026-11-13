//! Model adaptor operations the settings UI needs: list the models a provider offers and
//! test a connection with one minimal real request.
//!
//! Both work for every [`ProviderKind`], so any OpenAI-compatible endpoint (OpenRouter, Groq,
//! Together, LM Studio, vLLM…) is covered by supplying its base URL. The key reaches this
//! module only through [`ProviderConfig`]; it is redacted from every message returned.

use super::{
    anthropic, build_provider_with, http, ApiKey, ProviderConfig, ProviderError, ProviderKind,
};
use crate::message::{CancelToken, ChatMessage, ChatRequest};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

/// Outcome of [`test_connection`]. Never contains the API key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionReport {
    pub ok: bool,
    pub latency_ms: u64,
    /// On success a short confirmation; on failure the provider's (or transport's) error text.
    pub message: String,
}

fn redact(text: String, key: Option<&ApiKey>) -> String {
    match key.map(ApiKey::expose).filter(|k| !k.is_empty()) {
        Some(k) => text.replace(k, "<redacted>"),
        None => text,
    }
}

fn auth_headers(kind: ProviderKind, key: Option<&ApiKey>) -> Vec<(String, String)> {
    let Some(key) = key else { return vec![] };
    match kind {
        ProviderKind::Anthropic => vec![
            ("x-api-key".into(), key.expose().to_string()),
            ("anthropic-version".into(), anthropic::API_VERSION.into()),
        ],
        _ => vec![("authorization".into(), format!("Bearer {}", key.expose()))],
    }
}

/// Model ids the provider reports, sorted and de-duplicated. The caller falls back to a
/// free-text model id when this fails (some endpoints have no listing).
pub fn list_models(
    config: &ProviderConfig,
    transport: Arc<dyn http::HttpTransport>,
    cancel: &CancelToken,
) -> Result<Vec<String>, ProviderError> {
    let key = config.resolve_api_key()?;
    let ep = http::Endpoint {
        base_url: config.base_url(),
        model: String::new(),
        api_key: key.clone(),
        timeout: config.timeout(),
        max_retries: 0,
        transport,
    };
    let (path, ids_at, id_field) = match config.kind {
        ProviderKind::OpenAiCompatible => ("/models", "data", "id"),
        ProviderKind::Anthropic => ("/v1/models?limit=1000", "data", "id"),
        ProviderKind::Ollama => ("/api/tags", "models", "name"),
    };
    let v = ep
        .get_json(path, auth_headers(config.kind, key.as_ref()), cancel)
        .map_err(|e| redact_error(e, key.as_ref()))?;
    let mut ids: Vec<String> = v
        .get(ids_at)
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Malformed("model list has no `data`/`models` array".into()))?
        .iter()
        .filter_map(|m| m.get(id_field).and_then(Value::as_str).map(String::from))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn redact_error(e: ProviderError, key: Option<&ApiKey>) -> ProviderError {
    match e {
        ProviderError::Auth { status, message } => ProviderError::Auth {
            status,
            message: redact(message, key),
        },
        ProviderError::Http { status, message } => ProviderError::Http {
            status,
            message: redact(message, key),
        },
        ProviderError::Server { status, message } => ProviderError::Server {
            status,
            message: redact(message, key),
        },
        ProviderError::RateLimited(m) => ProviderError::RateLimited(redact(m, key)),
        ProviderError::Transport(m) => ProviderError::Transport(redact(m, key)),
        ProviderError::Malformed(m) => ProviderError::Malformed(redact(m, key)),
        other => other,
    }
}

/// Send one minimal real chat request (a few output tokens, no retries) and report the
/// result. A missing model or key is reported as a failure, never as success.
pub fn test_connection(
    config: &ProviderConfig,
    transport: Arc<dyn http::HttpTransport>,
    cancel: &CancelToken,
) -> ConnectionReport {
    let started = Instant::now();
    let key = config.resolve_api_key().ok().flatten();
    let outcome = (|| {
        let mut cfg = config.clone();
        cfg.max_retries = 0;
        let provider = build_provider_with(&cfg, transport)?;
        let request = ChatRequest {
            system: "Connection test from EAK. Answer as briefly as possible.".into(),
            messages: vec![ChatMessage::User {
                text: "Reply with the single word OK.".into(),
            }],
            tools: vec![],
            max_tokens: 16,
        };
        provider
            .complete(&request, cancel)
            .map(|r| (provider.label(), r.text))
    })();
    let latency_ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok((label, text)) => ConnectionReport {
            ok: true,
            latency_ms,
            message: redact(
                format!("Connected to {label}; the model replied: {}", text.trim()),
                key.as_ref(),
            ),
        },
        Err(e) => ConnectionReport {
            ok: false,
            latency_ms,
            message: redact(e.to_string(), key.as_ref()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::http::testing::RecordingTransport;

    fn cfg(kind: ProviderKind, key: Option<&str>) -> ProviderConfig {
        let mut c = ProviderConfig::new(kind);
        c.model = Some("m1".into());
        c.api_key = key.map(ApiKey::new);
        c
    }

    #[test]
    fn lists_models_for_each_provider_shape() {
        let t = RecordingTransport::with(vec![(
            200,
            r#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#,
        )]);
        let ids = list_models(
            &cfg(ProviderKind::OpenAiCompatible, Some("sk-1")),
            t.clone(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(ids, ["a", "b"]);
        let sent = t.requests.lock().unwrap()[0].clone();
        assert_eq!(sent.url, "https://api.openai.com/v1/models");
        assert!(sent
            .headers
            .contains(&("authorization".into(), "Bearer sk-1".into())));

        let t = RecordingTransport::with(vec![(200, r#"{"data":[{"id":"claude-x"}]}"#)]);
        let ids = list_models(
            &cfg(ProviderKind::Anthropic, Some("ak")),
            t.clone(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(ids, ["claude-x"]);
        let sent = t.requests.lock().unwrap()[0].clone();
        assert!(sent.url.ends_with("/v1/models?limit=1000"));
        assert!(sent.headers.contains(&("x-api-key".into(), "ak".into())));

        let t = RecordingTransport::with(vec![(200, r#"{"models":[{"name":"llama3:8b"}]}"#)]);
        let ids = list_models(
            &cfg(ProviderKind::Ollama, None),
            t.clone(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(ids, ["llama3:8b"]);
        assert!(t.requests.lock().unwrap()[0].headers.is_empty());
    }

    #[test]
    fn a_listing_without_models_is_malformed_not_empty_success() {
        let t = RecordingTransport::with(vec![(200, r#"{"object":"list"}"#)]);
        let e = list_models(
            &cfg(ProviderKind::OpenAiCompatible, Some("k")),
            t,
            &CancelToken::new(),
        )
        .unwrap_err();
        assert!(matches!(e, ProviderError::Malformed(_)), "{e}");
    }

    #[test]
    fn test_connection_reports_success_and_failure_text_without_the_key() {
        let ok = RecordingTransport::with(vec![(
            200,
            r#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#,
        )]);
        let r = test_connection(
            &cfg(ProviderKind::OpenAiCompatible, Some("sk-secret")),
            ok.clone(),
            &CancelToken::new(),
        );
        assert!(r.ok, "{r:?}");
        assert!(r.message.contains("OK"));
        assert_eq!(ok.requests.lock().unwrap().len(), 1);

        // The provider echoes the key in its error text: it must be redacted.
        let bad = RecordingTransport::with(vec![(
            401,
            r#"{"error":{"message":"Incorrect API key provided: sk-secret"}}"#,
        )]);
        let r = test_connection(
            &cfg(ProviderKind::OpenAiCompatible, Some("sk-secret")),
            bad,
            &CancelToken::new(),
        );
        assert!(!r.ok);
        assert!(r.message.contains("Incorrect API key"), "{}", r.message);
        assert!(!r.message.contains("sk-secret"), "{}", r.message);
    }

    #[test]
    fn missing_key_or_model_is_a_failure_never_success() {
        let t = RecordingTransport::with(vec![]);
        let r = test_connection(
            &cfg(ProviderKind::Anthropic, None),
            t.clone(),
            &CancelToken::new(),
        );
        assert!(!r.ok && r.message.contains("missing API key"), "{r:?}");
        let mut no_model = ProviderConfig::new(ProviderKind::Ollama);
        no_model.model = None;
        let r = test_connection(&no_model, t.clone(), &CancelToken::new());
        assert!(!r.ok && r.message.contains("model"), "{r:?}");
        assert!(t.requests.lock().unwrap().is_empty());
    }
}
