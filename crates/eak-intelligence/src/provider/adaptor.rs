//! Provider operations for settings and agent integrations: discover the models a provider
//! offers (with whatever metadata its listing carries) and validate a connection in stages.
//!
//! The key reaches this module only through [`ProviderConfig`]; it is redacted from every
//! message returned. A connection is reported `connected` only after a real model request
//! streamed a reply: saving settings, or a successful model listing alone, never counts.

use super::catalog::{ModelInfo, ModelSource};
use super::{
    auth_headers, build_provider_with, http, ApiKey, ErrorClass, ProviderConfig, ProviderError,
    ProviderKind,
};
use crate::message::{CancelToken, ChatMessage, ChatRequest, StreamEvent};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// A real model request streamed a reply.
    Connected,
    /// Credentials and endpoint work, but no model is chosen yet.
    Authenticated,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Settings are complete and well-formed.
    Configuration,
    /// Credentials and connectivity, checked through the model listing.
    Authentication,
    /// The chosen model is offered by the provider.
    Model,
    /// One minimal real streaming request.
    Invocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StageReport {
    pub stage: Stage,
    /// `None` = skipped (with the reason in `message`).
    pub ok: Option<bool>,
    pub message: String,
}

/// Outcome of [`test_connection`]. Never contains the API key.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectionReport {
    pub state: ConnectionState,
    /// `state == Connected`.
    pub ok: bool,
    pub latency_ms: u64,
    pub message: String,
    pub error_class: Option<ErrorClass>,
    pub stages: Vec<StageReport>,
    /// Models discovered during the test (empty when the listing is unavailable).
    pub models: Vec<ModelInfo>,
}

fn redact(text: String, key: Option<&ApiKey>) -> String {
    match key.map(ApiKey::expose).filter(|k| !k.is_empty()) {
        Some(k) => text.replace(k, "<redacted>"),
        None => text,
    }
}

fn redact_error(e: ProviderError, key: Option<&ApiKey>) -> ProviderError {
    use ProviderError::*;
    match e {
        Auth { status, message } => Auth {
            status,
            message: redact(message, key),
        },
        Http { status, message } => Http {
            status,
            message: redact(message, key),
        },
        Server { status, message } => Server {
            status,
            message: redact(message, key),
        },
        RateLimited(m) => RateLimited(redact(m, key)),
        Transport(m) => Transport(redact(m, key)),
        Malformed(m) => Malformed(redact(m, key)),
        InvalidBaseUrl(m) => InvalidBaseUrl(redact(m, key)),
        other => other,
    }
}

fn u64_at(v: &Value, paths: &[&str]) -> Option<u64> {
    paths
        .iter()
        .find_map(|p| v.pointer(p).and_then(Value::as_u64))
}

fn str_at<'a>(v: &'a Value, paths: &[&str]) -> Option<&'a str> {
    paths
        .iter()
        .find_map(|p| v.pointer(p).and_then(Value::as_str))
}

/// Read one model entry of any supported listing shape, keeping only metadata the provider
/// actually reported.
fn model_entry(kind: ProviderKind, m: &Value) -> Option<ModelInfo> {
    let id = match kind {
        ProviderKind::Ollama => m["name"].as_str()?,
        ProviderKind::Gemini => m["name"].as_str()?.trim_start_matches("models/"),
        _ => m["id"].as_str()?,
    };
    let mut info = ModelInfo::bare(id, ModelSource::Discovered);
    if kind != ProviderKind::Ollama {
        info.name = str_at(m, &["/display_name", "/displayName", "/name"])
            .filter(|n| *n != id && !n.starts_with("models/"))
            .map(String::from);
    }
    info.context_window = u64_at(
        m,
        &[
            "/context_length",
            "/context_window",
            "/max_context_length",
            "/inputTokenLimit",
            "/max_input_tokens",
            "/top_provider/context_length",
        ],
    );
    info.max_output_tokens = u64_at(
        m,
        &[
            "/outputTokenLimit",
            "/top_provider/max_completion_tokens",
            "/max_completion_tokens",
            "/max_tokens",
        ],
    );
    if let Some(params) = m["supported_parameters"].as_array() {
        info.tools = Some(params.iter().any(|p| p == "tools"));
    } else if let Some(b) = m
        .pointer("/capabilities/function_calling")
        .and_then(Value::as_bool)
    {
        info.tools = Some(b);
    }
    if let Some(mods) = m
        .pointer("/architecture/input_modalities")
        .and_then(Value::as_array)
    {
        info.vision = Some(mods.iter().any(|x| x == "image"));
    } else if let Some(b) = m.pointer("/capabilities/vision").and_then(Value::as_bool) {
        info.vision = Some(b);
    }
    Some(info)
}

/// Parse a model listing into [`ModelInfo`]s, sorted by id and de-duplicated. Entries that
/// are clearly not chat models (embeddings, images, audio…) are left out.
pub fn parse_model_list(kind: ProviderKind, v: &Value) -> Result<Vec<ModelInfo>, ProviderError> {
    let arr = match kind {
        ProviderKind::Ollama | ProviderKind::Gemini => v.get("models"),
        // Together answers a bare array; everyone else `{ "data": [...] }`.
        _ => v.get("data").or(Some(v)),
    }
    .and_then(Value::as_array)
    .ok_or_else(|| ProviderError::Malformed("model list has no `data`/`models` array".into()))?;
    let mut out: Vec<ModelInfo> = arr
        .iter()
        .filter(|m| match kind {
            ProviderKind::Gemini => m["supportedGenerationMethods"]
                .as_array()
                .is_none_or(|a| a.iter().any(|x| x == "generateContent")),
            _ => !matches!(
                m["type"].as_str(),
                Some("embedding" | "image" | "moderation" | "rerank" | "audio" | "transcribe")
            ),
        })
        .filter_map(|m| model_entry(kind, m))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

fn listing_path(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "/models",
        ProviderKind::Anthropic => "/v1/models?limit=1000",
        ProviderKind::Ollama => "/api/tags",
        ProviderKind::Gemini => "/models?pageSize=1000",
    }
}

/// Models the provider reports. The caller falls back to typing a model id when this fails
/// (some endpoints have no listing).
pub fn list_models(
    config: &ProviderConfig,
    transport: Arc<dyn http::HttpTransport>,
    cancel: &CancelToken,
) -> Result<Vec<ModelInfo>, ProviderError> {
    config.validate_base_url()?;
    let key = config.resolve_api_key()?;
    let ep = http::Endpoint {
        base_url: config.base_url(),
        model: String::new(),
        api_key: key.clone(),
        timeout: config.timeout(),
        max_retries: 0,
        extra_headers: config.extra_headers.clone(),
        transport,
    };
    ep.get_json(
        listing_path(config.kind),
        auth_headers(config.kind, key.as_ref()),
        cancel,
    )
    .and_then(|v| parse_model_list(config.kind, &v))
    .map_err(|e| redact_error(e, key.as_ref()))
}

fn same_model(kind: ProviderKind, listed: &str, wanted: &str) -> bool {
    let w = wanted.trim();
    match kind {
        ProviderKind::Ollama => {
            let norm = |s: &str| s.strip_suffix(":latest").unwrap_or(s).to_string();
            norm(listed) == norm(w)
        }
        ProviderKind::Gemini => listed == w.trim_start_matches("models/"),
        _ => listed == w,
    }
}

/// Listing failures that mean "this endpoint has no listing", not "the connection is
/// broken": the invocation stage still decides.
fn listing_unsupported(e: &ProviderError) -> bool {
    matches!(
        e.class(),
        ErrorClass::InvalidBaseUrl | ErrorClass::RequestRejected | ErrorClass::InvalidResponse
    )
}

fn stage(stage: Stage, ok: Option<bool>, message: String) -> StageReport {
    StageReport { stage, ok, message }
}

/// Validate a provider in stages (Spec 5.4): configuration, authentication and connectivity
/// (through the model listing), model availability, and one minimal real streaming request
/// (a few output tokens, no retries). Failure is data in the report, never an `Err`.
pub fn test_connection(
    config: &ProviderConfig,
    transport: Arc<dyn http::HttpTransport>,
    cancel: &CancelToken,
) -> ConnectionReport {
    let started = Instant::now();
    let key = config.resolve_api_key().ok().flatten();
    let elapsed = || started.elapsed().as_millis() as u64;
    let fail = |stages: Vec<StageReport>, models: Vec<ModelInfo>, e: ProviderError| {
        let e = redact_error(e, key.as_ref());
        let class = e.class();
        ConnectionReport {
            state: ConnectionState::Failed,
            ok: false,
            latency_ms: elapsed(),
            message: format!("{}: {e}", class.title()),
            error_class: Some(class),
            stages,
            models,
        }
    };
    let mut stages = vec![];
    let mut models = vec![];

    // 1. Configuration: nothing is sent for incomplete or malformed settings.
    if let Err(e) = config
        .validate_base_url()
        .and_then(|_| config.resolve_api_key().map(|_| ()))
    {
        stages.push(stage(Stage::Configuration, Some(false), e.to_string()));
        return fail(stages, models, e);
    }
    stages.push(stage(
        Stage::Configuration,
        Some(true),
        format!("endpoint {}", config.base_url()),
    ));

    // 2. Authentication and connectivity through the model listing.
    let mut listed = false;
    match list_models(config, transport.clone(), cancel) {
        Ok(list) => {
            listed = true;
            stages.push(stage(
                Stage::Authentication,
                Some(true),
                format!("credentials accepted; {} model(s) listed", list.len()),
            ));
            models = list;
        }
        Err(e) if listing_unsupported(&e) => stages.push(stage(
            Stage::Authentication,
            None,
            format!("model listing unavailable ({e}); the request below checks the credentials"),
        )),
        Err(e) => {
            stages.push(stage(Stage::Authentication, Some(false), e.to_string()));
            return fail(stages, models, e);
        }
    }

    // 3. Model availability.
    let chosen = config
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(String::from)
        .or_else(|| config.kind.default_model().map(String::from));
    let Some(model) = chosen else {
        stages.push(stage(Stage::Model, None, "no model chosen".into()));
        if listed {
            return ConnectionReport {
                state: ConnectionState::Authenticated,
                ok: false,
                latency_ms: elapsed(),
                message: "Authenticated. Choose a model to finish the connection test.".into(),
                error_class: None,
                stages,
                models,
            };
        }
        return fail(
            stages,
            models,
            ProviderError::Config("choose a model to test the connection".into()),
        );
    };
    if listed && !models.is_empty() {
        if models
            .iter()
            .any(|m| same_model(config.kind, &m.id, &model))
        {
            stages.push(stage(
                Stage::Model,
                Some(true),
                format!("{model} is offered"),
            ));
        } else {
            let e = ProviderError::Http {
                status: 404,
                message: format!("model {model} is not in this provider's model list"),
            };
            stages.push(stage(Stage::Model, Some(false), e.to_string()));
            return fail(stages, models, e);
        }
    } else {
        stages.push(stage(
            Stage::Model,
            None,
            "not listed by the provider; the request below checks it".into(),
        ));
    }

    // 4. One real streaming request.
    let mut cfg = config.clone();
    cfg.max_retries = 0;
    cfg.model = Some(model);
    let outcome = build_provider_with(&cfg, transport).and_then(|provider| {
        let request = ChatRequest {
            system: "Connection test from EAK. Answer as briefly as possible.".into(),
            messages: vec![ChatMessage::User {
                text: "Reply with the single word OK.".into(),
            }],
            tools: vec![],
            max_tokens: 16,
        };
        let mut chunks = 0u32;
        let r = provider.stream(&request, cancel, &mut |e: StreamEvent| {
            if matches!(
                e,
                StreamEvent::TextDelta { .. } | StreamEvent::ThinkingDelta { .. }
            ) {
                chunks += 1;
            }
        })?;
        Ok((provider.label(), r.text, chunks))
    });
    match outcome {
        Ok((label, text, chunks)) => {
            let message = redact(
                format!(
                    "Connected to {label}; streamed {chunks} chunk(s); the model replied: {}",
                    text.trim()
                ),
                key.as_ref(),
            );
            stages.push(stage(Stage::Invocation, Some(true), message.clone()));
            ConnectionReport {
                state: ConnectionState::Connected,
                ok: true,
                latency_ms: elapsed(),
                message,
                error_class: None,
                stages,
                models,
            }
        }
        Err(e) => {
            let e = redact_error(e, key.as_ref());
            stages.push(stage(Stage::Invocation, Some(false), e.to_string()));
            fail(stages, models, e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::http::testing::RecordingTransport;
    use serde_json::json;

    fn cfg(kind: ProviderKind, key: Option<&str>) -> ProviderConfig {
        let mut c = ProviderConfig::new(kind);
        c.model = Some("m1".into());
        c.api_key = key.map(ApiKey::new);
        c
    }

    const OK_STREAM: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

    #[test]
    fn listing_metadata_is_read_where_the_provider_reports_it() {
        // OpenRouter-style metadata.
        let v = json!({"data": [
            {"id": "b/x", "name": "X", "context_length": 200000,
             "top_provider": {"max_completion_tokens": 8192},
             "supported_parameters": ["tools", "temperature"],
             "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "a/embed", "type": "embedding"},
            {"id": "a/y"}
        ]});
        let m = parse_model_list(ProviderKind::OpenAiCompatible, &v).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], ModelInfo::bare("a/y", ModelSource::Discovered));
        assert_eq!(m[1].name.as_deref(), Some("X"));
        assert_eq!(m[1].context_window, Some(200000));
        assert_eq!(m[1].max_output_tokens, Some(8192));
        assert_eq!((m[1].tools, m[1].vision), (Some(true), Some(true)));
        // Gemini: drop embedding-only models, strip `models/`.
        let g = json!({"models": [
            {"name": "models/gemini-a", "displayName": "Gemini A", "inputTokenLimit": 1048576,
             "outputTokenLimit": 65536, "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/embed", "supportedGenerationMethods": ["embedContent"]}
        ]});
        let m = parse_model_list(ProviderKind::Gemini, &g).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, "gemini-a");
        assert_eq!(m[0].name.as_deref(), Some("Gemini A"));
        assert_eq!(m[0].context_window, Some(1048576));
        // Together: bare array.
        let t = json!([{"id": "t1", "type": "chat", "context_length": 32768}]);
        assert_eq!(
            parse_model_list(ProviderKind::OpenAiCompatible, &t).unwrap()[0].context_window,
            Some(32768)
        );
        // Mistral capability flags.
        let mi = json!({"data": [{"id": "mi", "max_context_length": 128000,
            "capabilities": {"function_calling": true, "vision": false}}]});
        let m = parse_model_list(ProviderKind::OpenAiCompatible, &mi).unwrap();
        assert_eq!((m[0].tools, m[0].vision), (Some(true), Some(false)));
        assert!(
            parse_model_list(ProviderKind::OpenAiCompatible, &json!({"object": "list"})).is_err()
        );
    }

    #[test]
    fn connected_only_after_a_streamed_reply() {
        let t =
            RecordingTransport::with(vec![(200, r#"{"data":[{"id":"m1"}]}"#), (200, OK_STREAM)]);
        let r = test_connection(
            &cfg(ProviderKind::OpenAiCompatible, Some("sk-1")),
            t.clone(),
            &CancelToken::new(),
        );
        assert_eq!(r.state, ConnectionState::Connected, "{r:?}");
        assert!(
            r.ok && r.message.contains("streamed 1 chunk"),
            "{}",
            r.message
        );
        assert_eq!(r.stages.len(), 4);
        assert!(r.stages.iter().all(|s| s.ok == Some(true)));
        assert_eq!(r.models.len(), 1);
        assert_eq!(t.bodies()[1]["stream"], true);
    }

    #[test]
    fn each_failure_has_its_class_and_never_reports_connected() {
        type Case = (Vec<(u16, &'static str)>, ErrorClass);
        let cases: Vec<Case> = vec![
            (
                vec![(
                    401,
                    r#"{"error":{"message":"Incorrect API key provided: sk-secret"}}"#,
                )],
                ErrorClass::InvalidApiKey,
            ),
            (
                vec![(403, r#"{"error":{"message":"forbidden"}}"#)],
                ErrorClass::AuthenticationFailed,
            ),
            (
                vec![(429, r#"{"error":{"message":"slow down"}}"#)],
                ErrorClass::RateLimited,
            ),
            (
                vec![(503, r#"{"error":{"message":"down"}}"#)],
                ErrorClass::ProviderUnavailable,
            ),
            (
                vec![(200, r#"{"data":[{"id":"other"}]}"#)],
                ErrorClass::UnsupportedModel,
            ),
            // No listing (404 page) and the chat path 404s too: wrong base URL.
            (
                vec![
                    (404, "<html>Not Found</html>"),
                    (404, "<html>Not Found</html>"),
                ],
                ErrorClass::InvalidBaseUrl,
            ),
            // No listing, and the request names an unknown model.
            (
                vec![
                    (404, "nope"),
                    (
                        404,
                        r#"{"error":{"message":"The model `m1` does not exist"}}"#,
                    ),
                ],
                ErrorClass::UnsupportedModel,
            ),
        ];
        for (responses, class) in cases {
            let t = RecordingTransport::with(responses);
            let r = test_connection(
                &cfg(ProviderKind::OpenAiCompatible, Some("sk-secret")),
                t,
                &CancelToken::new(),
            );
            assert_eq!(r.state, ConnectionState::Failed, "{r:?}");
            assert_eq!(r.error_class, Some(class), "{r:?}");
            assert!(!r.message.contains("sk-secret"), "{}", r.message);
            assert!(r.message.starts_with(class.title()), "{}", r.message);
        }
    }

    #[test]
    fn missing_key_bad_url_and_no_model() {
        let t = RecordingTransport::with(vec![]);
        let mut anth = cfg(ProviderKind::Anthropic, None);
        anth.api_key_env = Some("EAK_TEST_UNSET_KEY_VAR_9Z".into());
        let r = test_connection(&anth, t.clone(), &CancelToken::new());
        assert_eq!(r.error_class, Some(ErrorClass::MissingApiKey), "{r:?}");
        let mut bad = cfg(ProviderKind::OpenAiCompatible, None);
        bad.base_url = Some("localhost:8080".into());
        let r = test_connection(&bad, t.clone(), &CancelToken::new());
        assert_eq!(r.error_class, Some(ErrorClass::InvalidBaseUrl), "{r:?}");
        assert!(
            t.requests.lock().unwrap().is_empty(),
            "nothing is sent for bad settings"
        );

        // Credentials work but no model is chosen: authenticated, not connected.
        let t = RecordingTransport::with(vec![(200, r#"{"models":[{"name":"llama3:latest"}]}"#)]);
        let mut no_model = ProviderConfig::new(ProviderKind::Ollama);
        no_model.model = None;
        let r = test_connection(&no_model, t, &CancelToken::new());
        assert_eq!(r.state, ConnectionState::Authenticated, "{r:?}");
        assert!(!r.ok);
        assert!(same_model(ProviderKind::Ollama, "llama3:latest", "llama3"));
    }
}
