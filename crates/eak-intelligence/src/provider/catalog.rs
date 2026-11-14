//! Built-in provider catalog: every provider EAK knows by name, the wire protocol it speaks,
//! its fixed endpoint, how it authenticates and whether EAK implements it.
//!
//! Modelled on the provider registry of Pi (`packages/ai`, MIT): a provider is a named
//! endpoint + auth method + model list mapped onto a small set of protocol families
//! ([`ProviderKind`]). Known endpoints are fixed here so users never type a base URL for
//! them; only local servers and custom endpoints take one.
//!
//! Model metadata is deliberately sparse: built-in entries carry only facts EAK is sure of,
//! and everything else (context limits, capabilities) comes from the provider's own model
//! listing at discovery time. Unknown values stay `None` and the UI shows "unknown".

use super::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// A secret API key is required.
    ApiKey,
    /// Usually unauthenticated (local servers); a key may be supplied.
    OptionalApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Cloud,
    Local,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// Shipped with EAK.
    Catalog,
    /// Returned by the provider's model listing.
    Discovered,
}

/// What EAK knows about one model. `None` means unknown, never "no".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub tools: Option<bool>,
    #[serde(default)]
    pub vision: Option<bool>,
    pub source: ModelSource,
}

impl ModelInfo {
    pub fn bare(id: &str, source: ModelSource) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: None,
            context_window: None,
            max_output_tokens: None,
            tools: None,
            vision: None,
            source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub kind: ProviderKind,
    /// The provider's endpoint. Fixed for cloud providers; the usual default for local
    /// servers; `None` means the user must supply one (custom endpoints).
    pub base_url: Option<&'static str>,
    pub base_url_editable: bool,
    pub auth: AuthMethod,
    /// Offer OpenAI organization / project id fields (sent as headers).
    pub org_project: bool,
    /// The provider answers a model listing EAK can read.
    pub discovery: bool,
    /// False: listed for visibility but not usable yet (reason in `note`).
    pub implemented: bool,
    pub note: Option<&'static str>,
    /// Built-in model suggestions (discovery supplies the current list).
    pub models: Vec<ModelInfo>,
}

fn cloud(
    id: &'static str,
    name: &'static str,
    kind: ProviderKind,
    base_url: &'static str,
) -> ProviderSpec {
    ProviderSpec {
        id,
        name,
        group: Group::Cloud,
        kind,
        base_url: Some(base_url),
        base_url_editable: false,
        auth: AuthMethod::ApiKey,
        org_project: false,
        discovery: true,
        implemented: true,
        note: None,
        models: vec![],
    }
}

fn local(
    id: &'static str,
    name: &'static str,
    kind: ProviderKind,
    url: &'static str,
) -> ProviderSpec {
    ProviderSpec {
        group: Group::Local,
        base_url_editable: true,
        auth: AuthMethod::OptionalApiKey,
        ..cloud(id, name, kind, url)
    }
}

fn custom(
    id: &'static str,
    name: &'static str,
    kind: ProviderKind,
    auth: AuthMethod,
    note: &'static str,
) -> ProviderSpec {
    ProviderSpec {
        group: Group::Custom,
        base_url: None,
        base_url_editable: true,
        auth,
        note: Some(note),
        ..cloud(id, name, kind, "")
    }
}

fn not_implemented(id: &'static str, name: &'static str, note: &'static str) -> ProviderSpec {
    ProviderSpec {
        base_url: None,
        discovery: false,
        implemented: false,
        note: Some(note),
        ..cloud(id, name, ProviderKind::OpenAiCompatible, "")
    }
}

fn claude(id: &str, name: &str) -> ModelInfo {
    ModelInfo {
        name: Some(name.into()),
        tools: Some(true),
        ..ModelInfo::bare(id, ModelSource::Catalog)
    }
}

/// Every provider EAK lists, in display order.
pub fn providers() -> Vec<ProviderSpec> {
    use ProviderKind::*;
    vec![
        ProviderSpec {
            org_project: true,
            ..cloud("openai", "OpenAI", OpenAiCompatible, "https://api.openai.com/v1")
        },
        ProviderSpec {
            models: vec![
                claude("claude-opus-5-5", "Claude Opus 5.5"),
                claude("claude-sonnet-5-5", "Claude Sonnet 5.5"),
                claude("claude-haiku-5-5", "Claude Haiku 5.5"),
            ],
            ..cloud("anthropic", "Anthropic", Anthropic, "https://api.anthropic.com")
        },
        cloud("google", "Google Gemini", Gemini, "https://generativelanguage.googleapis.com/v1beta"),
        cloud("openrouter", "OpenRouter", OpenAiCompatible, "https://openrouter.ai/api/v1"),
        cloud("mistral", "Mistral", OpenAiCompatible, "https://api.mistral.ai/v1"),
        cloud("groq", "Groq", OpenAiCompatible, "https://api.groq.com/openai/v1"),
        cloud("xai", "xAI", OpenAiCompatible, "https://api.x.ai/v1"),
        cloud("deepseek", "DeepSeek", OpenAiCompatible, "https://api.deepseek.com"),
        cloud("cerebras", "Cerebras", OpenAiCompatible, "https://api.cerebras.ai/v1"),
        cloud("together", "Together AI", OpenAiCompatible, "https://api.together.ai/v1"),
        cloud("fireworks", "Fireworks AI", OpenAiCompatible, "https://api.fireworks.ai/inference/v1"),
        cloud("nvidia", "NVIDIA NIM", OpenAiCompatible, "https://integrate.api.nvidia.com/v1"),
        local("ollama", "Ollama (local)", Ollama, "http://localhost:11434"),
        local("lmstudio", "LM Studio (local)", OpenAiCompatible, "http://localhost:1234/v1"),
        custom(
            "openai-compatible",
            "Custom OpenAI-compatible endpoint",
            OpenAiCompatible,
            AuthMethod::OptionalApiKey,
            "Any server implementing the OpenAI /chat/completions API (vLLM, llama.cpp server, LiteLLM…).",
        ),
        custom(
            "anthropic-compatible",
            "Custom Anthropic-compatible endpoint",
            Anthropic,
            AuthMethod::ApiKey,
            "Any server implementing the Anthropic /v1/messages API.",
        ),
        not_implemented(
            "google-vertex",
            "Google Vertex AI",
            "Needs Google Cloud credentials (Application Default Credentials or a service account) and project/region routing, which EAK does not implement yet.",
        ),
        not_implemented(
            "amazon-bedrock",
            "Amazon Bedrock",
            "Needs AWS SigV4 request signing and the Bedrock Converse API, which EAK does not implement yet.",
        ),
        not_implemented(
            "azure-openai",
            "Azure OpenAI",
            "Needs Azure resource/deployment URLs and the api-key header scheme, which EAK does not implement or test yet.",
        ),
    ]
}

pub fn provider(id: &str) -> Option<ProviderSpec> {
    providers().into_iter().find(|p| p.id == id)
}

/// Catalog id for a profile saved before the catalog existed (kind + optional base URL).
pub fn infer(kind: ProviderKind, base_url: Option<&str>) -> &'static str {
    let base = base_url
        .map(|u| u.trim_end_matches('/'))
        .unwrap_or(kind.default_base_url());
    if let Some(p) = providers()
        .into_iter()
        .find(|p| p.implemented && p.kind == kind && p.base_url == Some(base))
    {
        return p.id;
    }
    match kind {
        ProviderKind::Anthropic => "anthropic-compatible",
        ProviderKind::Ollama => "ollama",
        ProviderKind::Gemini => "google",
        ProviderKind::OpenAiCompatible => "openai-compatible",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_the_compatibility_target_honestly() {
        let all = providers();
        for id in [
            "openai",
            "anthropic",
            "google",
            "openrouter",
            "mistral",
            "groq",
            "xai",
            "deepseek",
            "cerebras",
            "together",
            "fireworks",
            "nvidia",
            "ollama",
            "lmstudio",
            "openai-compatible",
            "anthropic-compatible",
        ] {
            let p = provider(id).unwrap_or_else(|| panic!("{id} missing"));
            assert!(p.implemented, "{id}");
        }
        for id in ["google-vertex", "amazon-bedrock", "azure-openai"] {
            let p = provider(id).unwrap();
            assert!(!p.implemented && p.note.is_some(), "{id} must say why");
        }
        // Known endpoints are fixed: no base URL typing for cloud providers.
        for p in all
            .iter()
            .filter(|p| p.group == Group::Cloud && p.implemented)
        {
            assert!(!p.base_url_editable, "{}", p.id);
            assert!(p.base_url.unwrap().starts_with("https://"), "{}", p.id);
        }
        assert!(provider("openai-compatible").unwrap().base_url.is_none());
        let mut ids: Vec<_> = all.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "duplicate provider ids");
    }

    #[test]
    fn legacy_profiles_map_onto_catalog_ids() {
        use ProviderKind::*;
        assert_eq!(infer(Anthropic, None), "anthropic");
        assert_eq!(
            infer(Anthropic, Some("https://api.anthropic.com/")),
            "anthropic"
        );
        assert_eq!(
            infer(Anthropic, Some("https://proxy.example")),
            "anthropic-compatible"
        );
        assert_eq!(
            infer(OpenAiCompatible, Some("https://openrouter.ai/api/v1")),
            "openrouter"
        );
        assert_eq!(infer(OpenAiCompatible, None), "openai");
        assert_eq!(
            infer(OpenAiCompatible, Some("http://10.0.0.2:8000/v1")),
            "openai-compatible"
        );
        assert_eq!(infer(Ollama, None), "ollama");
        assert_eq!(infer(Ollama, Some("http://box:11434")), "ollama");
    }
}
