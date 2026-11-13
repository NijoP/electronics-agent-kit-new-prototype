//! Model provider boundary (Execution Instructions §5 "model connections", §12 "optional
//! cloud: user-selected providers").
//!
//! A provider turns a [`ChatRequest`] into a [`ChatResponse`]. Adapters exist for
//! OpenAI-compatible chat completions, the Anthropic Messages API and Ollama. Providers are
//! constructed lazily from user configuration; nothing connects until the agent runs.
//!
//! API keys come only from the user's configuration value or an environment variable.
//! [`ApiKey`] is never serialised and its `Debug` output is redacted.

pub mod adaptor;
pub mod anthropic;
pub mod http;
pub mod ollama;
pub mod openai;

use crate::message::{CancelToken, ChatRequest, ChatResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("missing API key: {0}")]
    MissingApiKey(String),
    #[error("invalid provider configuration: {0}")]
    Config(String),
    #[error("provider rejected the credentials (HTTP {status}): {message}")]
    Auth { status: u16, message: String },
    #[error("provider rate limit (HTTP 429): {0}")]
    RateLimited(String),
    #[error("provider server error (HTTP {status}): {message}")]
    Server { status: u16, message: String },
    #[error("provider request failed (HTTP {status}): {message}")]
    Http { status: u16, message: String },
    #[error("remote HTTP transport not compiled in (build eak-intelligence with the `remote-http` feature); refused request to {0}")]
    TransportNotCompiledIn(String),
    #[error("network/transport error: {0}")]
    Transport(String),
    #[error("provider request timed out")]
    Timeout,
    #[error("malformed provider response: {0}")]
    Malformed(String),
    #[error("cancelled")]
    Cancelled,
    #[error("scripted provider exhausted: {0}")]
    Exhausted(String),
}

impl ProviderError {
    /// Transient failures worth a bounded retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::RateLimited(_)
                | ProviderError::Server { .. }
                | ProviderError::Transport(_)
                | ProviderError::Timeout
        )
    }
}

pub trait Provider: Send + Sync {
    /// Short label for logs, e.g. `anthropic:claude-opus-5-5`.
    fn label(&self) -> String;
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError>;
}

/// Secret API key. Deserialisable from user configuration; never serialised; redacted in
/// `Debug` so it cannot leak through logs or error messages.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(s: &str) -> ApiKey {
        ApiKey(s.trim().to_string())
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Any server implementing the OpenAI `/chat/completions` API (OpenAI, vLLM, LM Studio…).
    OpenAiCompatible,
    /// Anthropic Messages API.
    Anthropic,
    /// Ollama native `/api/chat` (local models).
    Ollama,
}

impl ProviderKind {
    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::OpenAiCompatible => openai::DEFAULT_BASE_URL,
            ProviderKind::Anthropic => anthropic::DEFAULT_BASE_URL,
            ProviderKind::Ollama => ollama::DEFAULT_BASE_URL,
        }
    }
    pub fn default_key_env(self) -> Option<&'static str> {
        match self {
            ProviderKind::OpenAiCompatible => Some("OPENAI_API_KEY"),
            ProviderKind::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderKind::Ollama => None,
        }
    }
    pub fn default_model(self) -> Option<&'static str> {
        match self {
            ProviderKind::Anthropic => Some(anthropic::DEFAULT_MODEL),
            // Model names on OpenAI-compatible servers and Ollama are deployment-specific.
            _ => None,
        }
    }
}

/// User-supplied provider configuration (from the user's settings, never from the repo).
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Key stored in the user's own configuration. Prefer `api_key_env`.
    #[serde(default)]
    pub api_key: Option<ApiKey>,
    /// Environment variable holding the key (defaults per provider kind).
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Bounded retries for rate limits, 5xx and transport errors.
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

fn default_timeout_secs() -> u64 {
    120
}
fn default_retries() -> u32 {
    2
}

impl ProviderConfig {
    pub fn new(kind: ProviderKind) -> ProviderConfig {
        ProviderConfig {
            kind,
            model: None,
            base_url: None,
            api_key: None,
            api_key_env: None,
            timeout_secs: default_timeout_secs(),
            max_retries: default_retries(),
        }
    }

    pub fn base_url(&self) -> String {
        self.base_url
            .as_deref()
            .unwrap_or(self.kind.default_base_url())
            .trim_end_matches('/')
            .to_string()
    }

    pub fn model(&self) -> Result<String, ProviderError> {
        match self.model.as_deref().map(str::trim) {
            Some(m) if !m.is_empty() => Ok(m.to_string()),
            _ => self.kind.default_model().map(String::from).ok_or_else(|| {
                ProviderError::Config(format!("{:?} needs a model name", self.kind))
            }),
        }
    }

    /// Whether this endpoint cannot work without a key. Local/self-hosted OpenAI-compatible
    /// servers and Ollama usually run unauthenticated.
    pub fn requires_key(&self) -> bool {
        match self.kind {
            ProviderKind::Anthropic => true,
            ProviderKind::OpenAiCompatible => self.base_url() == openai::DEFAULT_BASE_URL,
            ProviderKind::Ollama => false,
        }
    }

    /// Resolve the key: explicit configuration first, then the environment variable.
    pub fn resolve_api_key(&self) -> Result<Option<ApiKey>, ProviderError> {
        self.resolve_api_key_with(|name| std::env::var(name).ok())
    }

    /// As [`resolve_api_key`](Self::resolve_api_key) with an injectable environment lookup.
    pub fn resolve_api_key_with(
        &self,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Option<ApiKey>, ProviderError> {
        if let Some(k) = &self.api_key {
            if !k.expose().is_empty() {
                return Ok(Some(k.clone()));
            }
        }
        let var = self.api_key_env.as_deref().or(self.kind.default_key_env());
        if let Some(var) = var {
            if let Some(v) = env(var).filter(|v| !v.trim().is_empty()) {
                return Ok(Some(ApiKey::new(&v)));
            }
        }
        if self.requires_key() {
            return Err(ProviderError::MissingApiKey(format!(
                "set the {} environment variable or api_key in your user configuration",
                var.unwrap_or("<api_key_env>")
            )));
        }
        Ok(None)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs.max(1))
    }
}

/// Build a provider for the real endpoint. Live HTTPS needs the `remote-http` feature;
/// without it every request fails with `ProviderError::TransportNotCompiledIn`.
pub fn build_provider(config: &ProviderConfig) -> Result<Box<dyn Provider>, ProviderError> {
    build_provider_with(config, http::default_transport())
}

/// Build a provider over a caller-supplied transport (tests use a recording transport).
pub fn build_provider_with(
    config: &ProviderConfig,
    transport: Arc<dyn http::HttpTransport>,
) -> Result<Box<dyn Provider>, ProviderError> {
    let ep = http::Endpoint {
        base_url: config.base_url(),
        model: config.model()?,
        api_key: config.resolve_api_key()?,
        timeout: config.timeout(),
        max_retries: config.max_retries,
        transport,
    };
    Ok(match config.kind {
        ProviderKind::OpenAiCompatible => Box::new(openai::OpenAiProvider { ep }),
        ProviderKind::Anthropic => Box::new(anthropic::AnthropicProvider { ep }),
        ProviderKind::Ollama => Box::new(ollama::OllamaProvider { ep }),
    })
}

/// Parse a tool-call argument string; unparseable JSON is kept as a string so the tool
/// layer reports it to the model instead of guessing.
pub(crate) fn parse_arguments(s: &str) -> serde_json::Value {
    if s.trim().is_empty() {
        return serde_json::Value::Object(Default::default());
    }
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_resolution_order_and_requirements() {
        let mut c = ProviderConfig::new(ProviderKind::Anthropic);
        let env = |name: &str| (name == "ANTHROPIC_API_KEY").then(|| "env-key".to_string());
        assert_eq!(
            c.resolve_api_key_with(env).unwrap().unwrap().expose(),
            "env-key"
        );
        c.api_key = Some(ApiKey::new("cfg-key"));
        assert_eq!(
            c.resolve_api_key_with(env).unwrap().unwrap().expose(),
            "cfg-key"
        );
        c.api_key = None;
        c.api_key_env = Some("MY_KEY".into());
        assert!(matches!(
            c.resolve_api_key_with(env),
            Err(ProviderError::MissingApiKey(m)) if m.contains("MY_KEY")
        ));
        // A local OpenAI-compatible server and Ollama run without keys.
        let mut local = ProviderConfig::new(ProviderKind::OpenAiCompatible);
        local.base_url = Some("http://127.0.0.1:8080/v1".into());
        assert_eq!(local.resolve_api_key_with(|_| None).unwrap(), None);
        let official = ProviderConfig::new(ProviderKind::OpenAiCompatible);
        assert!(official.resolve_api_key_with(|_| None).is_err());
        assert_eq!(
            ProviderConfig::new(ProviderKind::Ollama)
                .resolve_api_key_with(|_| None)
                .unwrap(),
            None
        );
    }

    #[test]
    fn api_key_is_redacted_and_config_parses() {
        let c: ProviderConfig = serde_json::from_str(
            r#"{"kind":"open_ai_compatible","model":"m","api_key":"sk-secret-value"}"#,
        )
        .unwrap();
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("sk-secret-value"), "{dbg}");
        assert!(dbg.contains("<redacted>"));
        assert_eq!(c.timeout_secs, 120);
    }

    #[test]
    fn model_defaults() {
        assert_eq!(
            ProviderConfig::new(ProviderKind::Anthropic)
                .model()
                .unwrap(),
            "claude-opus-5-5"
        );
        assert!(ProviderConfig::new(ProviderKind::Ollama).model().is_err());
    }
}
