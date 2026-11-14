//! Model providers: optional infrastructure for EAK-native agent integrations. Nothing in the
//! EDA tools or the terminal needs it, and it never reads or touches the credentials or
//! configuration of external CLIs (pi, opencode, claude…), which keep their own auth.
//!
//! * Provider types come from the catalog in `eak_intelligence::provider::catalog`: known
//!   cloud endpoints are fixed (no base-URL typing), local servers have editable defaults,
//!   custom OpenAI-/Anthropic-compatible endpoints take a URL, and providers EAK has not
//!   implemented are refused with the catalog's reason.
//! * Profiles (name, provider, endpoint, model, OpenAI org/project ids, last connection
//!   status, discovered models) are plain JSON in the app config directory. They never
//!   contain a key. Export/import uses the same data minus status: portable, secret-free.
//! * API keys live **only** in the operating-system credential store (Windows Credential
//!   Manager, macOS Keychain, Secret Service on Linux) through [`OsKeyStore`]. They are never
//!   written to a file, the IR or a log, and never returned to the UI (it sees `has_key`).
//! * A profile is "connected" only after `test_connection` streamed a real model reply;
//!   saving or editing resets the status to untested.
//! * [`ModelSettings::resolve`] is the runtime entry point for agent integrations: a ready
//!   [`Provider`](eak_intelligence::Provider) for a profile (or the default one), with an
//!   optional per-call model override.
//!
//! Everything is a plain Rust function over [`ModelSettings`] so it is tested headless; `cmd`
//! holds the thin Tauri wrappers.

use eak_intelligence::message::CancelToken;
use eak_intelligence::provider::adaptor::{self, ConnectionReport, ConnectionState};
use eak_intelligence::provider::catalog::{self, AuthMethod, ModelInfo, ProviderSpec};
use eak_intelligence::provider::http::{default_transport, HttpTransport};
use eak_intelligence::provider::{ApiKey, ErrorClass, ProviderConfig, ProviderKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const KEYRING_SERVICE: &str = "eak-desktop.model-provider";

/// Where secrets go. The production implementation is the OS credential store.
pub trait KeyStore: Send + Sync {
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    /// Deleting a key that does not exist is not an error.
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// The operating-system credential store (`keyring` v3).
pub struct OsKeyStore;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|e| format!("credential store: {e}"))
}

impl KeyStore for OsKeyStore {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        entry(id)?
            .set_password(secret)
            .map_err(|e| format!("credential store refused the key: {e}"))
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match entry(id)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("credential store unavailable: {e}")),
        }
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("credential store unavailable: {e}")),
        }
    }
}

/// In-process store for tests that must not touch the real credential store.
#[derive(Default)]
pub struct MemoryKeyStore(Mutex<HashMap<String, String>>);

impl KeyStore for MemoryKeyStore {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "poisoned".to_string())?
            .insert(id.into(), secret.into());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "poisoned".to_string())?
            .get(id)
            .cloned())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "poisoned".to_string())?
            .remove(id);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    /// Saved but never tested since the last change.
    #[default]
    Untested,
    Connected,
    Authenticated,
    Failed,
}

/// Result of the last connection test. Reset to untested whenever the profile changes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ConnectionStatus {
    #[serde(default)]
    pub state: StatusState,
    #[serde(default)]
    pub error_class: Option<ErrorClass>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub checked_at_ms: Option<u64>,
    /// Model the test ran against.
    #[serde(default)]
    pub model: Option<String>,
}

/// A saved provider. Holds no secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub label: String,
    /// Catalog provider id (`openai`, `ollama`, `openai-compatible`…). Profiles saved before
    /// the catalog existed have none; it is inferred from `kind` and `base_url` on load.
    #[serde(default)]
    pub provider: String,
    pub kind: ProviderKind,
    /// Only for local servers and custom endpoints; cloud endpoints come from the catalog.
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Models found by the last discovery (not secret; shown by the model selector).
    #[serde(default)]
    pub models: Vec<ModelInfo>,
    #[serde(default)]
    pub status: ConnectionStatus,
}

/// What the UI sends to add or edit a provider (`id` empty or missing = new).
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileInput {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    /// Catalog provider id. When absent it is inferred from `kind` + `base_url`.
    #[serde(default)]
    pub provider: Option<String>,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

impl ProfileInput {
    /// A new profile of catalog provider `provider` (kind taken from the catalog on save).
    pub fn new(label: &str, provider: &str) -> ProfileInput {
        ProfileInput {
            id: None,
            label: label.into(),
            provider: Some(provider.into()),
            kind: catalog::provider(provider)
                .map(|s| s.kind)
                .unwrap_or(ProviderKind::OpenAiCompatible),
            base_url: None,
            model: None,
            org_id: None,
            project_id: None,
        }
    }
}

/// What the UI sees: the profile plus key presence and catalog facts. Never the key.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub profile: ProviderProfile,
    pub provider_name: String,
    /// The endpoint requests go to (catalog endpoint or the profile's own).
    pub endpoint: String,
    pub has_key: bool,
    /// False for endpoints that normally run unauthenticated (Ollama, local servers).
    pub needs_key: bool,
    /// The credential store could not be read for this provider (shown, not hidden).
    pub key_error: Option<String>,
    pub is_default: bool,
}

/// On-disk format (version 2). Version 1 was a bare array of profiles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    providers: Vec<ProviderProfile>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AnyFile {
    V2(SettingsFile),
    V1(Vec<ProviderProfile>),
}

/// Portable export: profiles without status or secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableConfig {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub default_provider: Option<String>,
    pub providers: Vec<PortableProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableProvider {
    /// Only used to keep `default_provider` pointing at the right entry.
    pub id: String,
    pub label: String,
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

const EXPORT_FORMAT: &str = "eak-model-providers";

pub struct ModelSettings {
    file: Option<PathBuf>,
    state: Mutex<SettingsFile>,
    keys: Arc<dyn KeyStore>,
    transport: Arc<dyn HttpTransport>,
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn spec_of(p: &ProviderProfile) -> Option<ProviderSpec> {
    catalog::provider(&p.provider)
}

/// The endpoint a profile talks to.
fn endpoint_of(p: &ProviderProfile) -> String {
    let spec = spec_of(p);
    let fixed = spec
        .as_ref()
        .filter(|s| !s.base_url_editable)
        .and_then(|s| s.base_url);
    fixed
        .map(String::from)
        .or_else(|| p.base_url.clone())
        .or_else(|| spec.and_then(|s| s.base_url).map(String::from))
        .unwrap_or_else(|| p.kind.default_base_url().to_string())
}

/// Environment variable that is never set. Pointing `api_key_env` at it stops
/// `eak-intelligence` falling back to `OPENAI_API_KEY`/`ANTHROPIC_API_KEY`, so a key can only
/// come from the credential store.
const NO_ENV_KEY: &str = "EAK_KEYS_ARE_IN_THE_OS_CREDENTIAL_STORE_ONLY";

fn config_for(p: &ProviderProfile, key: Option<String>) -> ProviderConfig {
    let spec = spec_of(p);
    let mut c = ProviderConfig::new(p.kind);
    c.base_url = Some(endpoint_of(p));
    c.model = p.model.clone();
    c.api_key = key.as_deref().map(ApiKey::new);
    c.api_key_env = Some(NO_ENV_KEY.into());
    c.key_required = spec.as_ref().map(|s| s.auth == AuthMethod::ApiKey);
    if spec.as_ref().is_some_and(|s| s.org_project) {
        if let Some(o) = &p.org_id {
            c.extra_headers
                .push(("OpenAI-Organization".into(), o.clone()));
        }
        if let Some(pr) = &p.project_id {
            c.extra_headers.push(("OpenAI-Project".into(), pr.clone()));
        }
    }
    c.timeout_secs = 60;
    c
}

fn new_id(existing: &[ProviderProfile]) -> String {
    let mut n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    loop {
        let id = format!("prov-{n:x}");
        if !existing.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

/// Validate one provider definition against the catalog and normalise it.
fn normalise(
    label: &str,
    provider: Option<String>,
    kind: ProviderKind,
    base_url: Option<String>,
) -> Result<(String, ProviderSpec, Option<String>), String> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err("give the provider a name".into());
    }
    let base_url = clean(base_url).map(|u| u.trim_end_matches('/').to_string());
    let provider_id =
        clean(provider).unwrap_or_else(|| catalog::infer(kind, base_url.as_deref()).to_string());
    let spec = catalog::provider(&provider_id)
        .ok_or_else(|| format!("unknown provider type {provider_id}"))?;
    if !spec.implemented {
        return Err(format!(
            "{} is not implemented in EAK yet: {}",
            spec.name,
            spec.note.unwrap_or_default()
        ));
    }
    let base_url = if spec.base_url_editable {
        match base_url {
            Some(u) => {
                let mut probe = ProviderConfig::new(spec.kind);
                probe.base_url = Some(u.clone());
                probe.validate_base_url().map_err(|e| e.to_string())?;
                Some(u)
            }
            None if spec.base_url.is_none() => {
                return Err(format!("{} needs a base URL", spec.name))
            }
            None => None,
        }
    } else {
        None // fixed catalog endpoint
    };
    Ok((label, spec, base_url))
}

impl ModelSettings {
    /// `file` is the settings JSON (None = not persisted). A corrupt file is reported, never
    /// silently replaced. Version-1 files (a bare array) load and are upgraded on next save.
    pub fn new(
        file: Option<PathBuf>,
        keys: Arc<dyn KeyStore>,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<ModelSettings, String> {
        let mut state = match &file {
            Some(f) if f.exists() => {
                let text = std::fs::read_to_string(f)
                    .map_err(|e| format!("cannot read {}: {e}", f.display()))?;
                match serde_json::from_str::<AnyFile>(&text)
                    .map_err(|e| format!("{} is not valid model settings: {e}", f.display()))?
                {
                    AnyFile::V2(s) => s,
                    AnyFile::V1(providers) => SettingsFile {
                        version: 2,
                        default_provider: None,
                        providers,
                    },
                }
            }
            _ => SettingsFile::default(),
        };
        state.version = 2;
        for p in &mut state.providers {
            if catalog::provider(&p.provider).is_none() {
                p.provider = catalog::infer(p.kind, p.base_url.as_deref()).to_string();
            }
        }
        Ok(ModelSettings {
            file,
            state: Mutex::new(state),
            keys,
            transport,
        })
    }

    /// Production wiring: OS credential store and the real HTTPS transport.
    pub fn system(file: Option<PathBuf>) -> Result<ModelSettings, String> {
        ModelSettings::new(file, Arc::new(OsKeyStore), default_transport())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, SettingsFile>, String> {
        self.state
            .lock()
            .map_err(|_| "model settings are poisoned; restart EAK".to_string())
    }

    fn persist(&self, s: &SettingsFile) -> Result<(), String> {
        let Some(f) = &self.file else { return Ok(()) };
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let tmp = f.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, f).map_err(|e| format!("cannot replace {}: {e}", f.display()))
    }

    /// Apply `f` to a copy of the settings, persist it, then commit it in memory.
    fn update<T>(
        &self,
        f: impl FnOnce(&mut SettingsFile) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.lock()?;
        let mut next = guard.clone();
        let out = f(&mut next)?;
        self.persist(&next)?;
        *guard = next;
        Ok(out)
    }

    fn view(&self, p: &ProviderProfile, default: Option<&str>) -> ProviderView {
        let (has_key, key_error) = match self.keys.get(&p.id) {
            Ok(k) => (k.is_some_and(|k| !k.trim().is_empty()), None),
            Err(e) => (false, Some(e)),
        };
        ProviderView {
            provider_name: spec_of(p).map(|s| s.name.to_string()).unwrap_or_default(),
            endpoint: endpoint_of(p),
            has_key,
            needs_key: config_for(p, None).requires_key(),
            key_error,
            is_default: default == Some(p.id.as_str()),
            profile: p.clone(),
        }
    }

    /// The provider catalog for the settings UI.
    pub fn catalog(&self) -> Vec<ProviderSpec> {
        catalog::providers()
    }

    pub fn list(&self) -> Result<Vec<ProviderView>, String> {
        let s = self.lock()?;
        let d = s.default_provider.as_deref();
        Ok(s.providers.iter().map(|p| self.view(p, d)).collect())
    }

    /// Add or update a provider; a non-empty `api_key` replaces the stored key. Any change
    /// resets the connection status to untested: saving never marks a provider connected.
    pub fn save(
        &self,
        input: ProfileInput,
        api_key: Option<String>,
    ) -> Result<ProviderView, String> {
        let (label, spec, base_url) =
            normalise(&input.label, input.provider, input.kind, input.base_url)?;
        let secret = api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from);
        let (profile, default) = self.update(|s| {
            let id = match clean(input.id) {
                Some(id) if s.providers.iter().any(|p| p.id == id) => id,
                Some(id) => return Err(format!("unknown provider {id}")),
                None => new_id(&s.providers),
            };
            let previous = s.providers.iter().find(|p| p.id == id).cloned();
            let mut profile = ProviderProfile {
                id: id.clone(),
                label,
                provider: spec.id.to_string(),
                kind: spec.kind,
                base_url,
                model: clean(input.model),
                org_id: clean(input.org_id).filter(|_| spec.org_project),
                project_id: clean(input.project_id).filter(|_| spec.org_project),
                models: vec![],
                status: ConnectionStatus::default(),
            };
            if let Some(prev) = previous {
                let same_endpoint = prev.provider == profile.provider
                    && prev.base_url == profile.base_url
                    && prev.org_id == profile.org_id
                    && prev.project_id == profile.project_id;
                if same_endpoint {
                    profile.models = prev.models;
                    if secret.is_none() && prev.model == profile.model {
                        profile.status = prev.status;
                    }
                }
            }
            if let Some(k) = &secret {
                self.keys.set(&id, k)?;
            }
            match s.providers.iter_mut().find(|p| p.id == id) {
                Some(slot) => *slot = profile.clone(),
                None => s.providers.push(profile.clone()),
            }
            // The first provider becomes the default so integrations have one to use.
            if s.default_provider.is_none() {
                s.default_provider = Some(id);
            }
            Ok((profile, s.default_provider.clone()))
        })?;
        Ok(self.view(&profile, default.as_deref()))
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        if !self.lock()?.providers.iter().any(|p| p.id == id) {
            return Err(format!("unknown provider {id}"));
        }
        // Key first: if the store refuses, the profile stays so the user can retry.
        self.keys.delete(id)?;
        self.update(|s| {
            s.providers.retain(|p| p.id != id);
            if s.default_provider.as_deref() == Some(id) {
                s.default_provider = s.providers.first().map(|p| p.id.clone());
            }
            Ok(())
        })
    }

    pub fn set_default(&self, id: &str) -> Result<Vec<ProviderView>, String> {
        self.update(|s| {
            if !s.providers.iter().any(|p| p.id == id) {
                return Err(format!("unknown provider {id}"));
            }
            s.default_provider = Some(id.to_string());
            Ok(())
        })?;
        self.list()
    }

    fn modify(
        &self,
        id: &str,
        f: impl FnOnce(&mut ProviderProfile),
    ) -> Result<ProviderView, String> {
        let (p, d) = self.update(|s| {
            let p = s
                .providers
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("unknown provider {id}"))?;
            f(p);
            Ok((p.clone(), s.default_provider.clone()))
        })?;
        Ok(self.view(&p, d.as_deref()))
    }

    pub fn set_key(&self, id: &str, api_key: &str) -> Result<ProviderView, String> {
        self.profile(id)?;
        let secret = api_key.trim();
        if secret.is_empty() {
            return Err("the key is empty; use remove key to delete it".into());
        }
        self.keys.set(id, secret)?;
        self.modify(id, |p| p.status = ConnectionStatus::default())
    }

    pub fn clear_key(&self, id: &str) -> Result<ProviderView, String> {
        self.profile(id)?;
        self.keys.delete(id)?;
        self.modify(id, |p| p.status = ConnectionStatus::default())
    }

    fn profile(&self, id: &str) -> Result<ProviderProfile, String> {
        self.lock()?
            .providers
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("unknown provider {id}"))
    }

    fn config(&self, id: &str) -> Result<ProviderConfig, String> {
        let p = self.profile(id)?;
        let key = self.keys.get(id)?.filter(|k| !k.trim().is_empty());
        Ok(config_for(&p, key))
    }

    /// Discover the provider's models (kept on the profile for the model selector). The UI
    /// falls back to typing a model id on `Err`.
    pub fn list_models(&self, id: &str) -> Result<Vec<ModelInfo>, String> {
        let cfg = self.config(id)?;
        let models = adaptor::list_models(&cfg, self.transport.clone(), &CancelToken::new())
            .map_err(|e| format!("{}: {e}", e.class().title()))?;
        let keep = models.clone();
        self.modify(id, |p| p.models = keep)?;
        Ok(models)
    }

    /// The staged connection test (configuration, authentication, model, one real streamed
    /// request). The outcome is stored as the profile's status. Always returns a report
    /// (failure is data, not an `Err`), except for an unknown provider id.
    pub fn test_connection(&self, id: &str) -> Result<ConnectionReport, String> {
        let cfg = self.config(id)?;
        let report = adaptor::test_connection(&cfg, self.transport.clone(), &CancelToken::new());
        let r = report.clone();
        self.modify(id, move |p| {
            if !r.models.is_empty() {
                p.models = r.models.clone();
            }
            p.status = ConnectionStatus {
                state: match r.state {
                    ConnectionState::Connected => StatusState::Connected,
                    ConnectionState::Authenticated => StatusState::Authenticated,
                    ConnectionState::Failed => StatusState::Failed,
                },
                error_class: r.error_class,
                message: Some(r.message.clone()),
                checked_at_ms: Some(now_ms()),
                model: p.model.clone(),
            };
        })?;
        Ok(report)
    }

    /// Runtime entry point for EAK-native agent integrations: a ready provider for profile
    /// `id` (or the default profile), optionally with another model of the same provider.
    /// Returns a plain reason when none is usable (no profile, no model, no key).
    pub fn resolve(
        &self,
        id: Option<&str>,
        model: Option<&str>,
    ) -> Result<Box<dyn eak_intelligence::Provider>, String> {
        let id = match id {
            Some(id) => id.to_string(),
            None => self
                .lock()?
                .default_provider
                .clone()
                .ok_or("no model provider is configured")?,
        };
        let mut cfg = self.config(&id)?;
        if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
            cfg.model = Some(m.to_string());
        }
        eak_intelligence::provider::build_provider_with(&cfg, self.transport.clone())
            .map_err(|e| format!("{}: {e}", e.class().title()))
    }

    /// A ready provider for profile `id` with its own model.
    pub fn provider(&self, id: &str) -> Result<Box<dyn eak_intelligence::Provider>, String> {
        self.resolve(Some(id), None)
    }

    /// Portable JSON of every profile: no keys, no connection status.
    pub fn export_config(&self) -> Result<String, String> {
        let s = self.lock()?;
        let cfg = PortableConfig {
            format: EXPORT_FORMAT.into(),
            version: 1,
            default_provider: s.default_provider.clone(),
            providers: s
                .providers
                .iter()
                .map(|p| PortableProvider {
                    id: p.id.clone(),
                    label: p.label.clone(),
                    provider: p.provider.clone(),
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    org_id: p.org_id.clone(),
                    project_id: p.project_id.clone(),
                })
                .collect(),
        };
        serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())
    }

    /// Add the providers of an exported configuration as new profiles (keys must be entered
    /// again on this machine). Everything is validated before anything is added.
    pub fn import_config(&self, json: &str) -> Result<Vec<ProviderView>, String> {
        let cfg: PortableConfig =
            serde_json::from_str(json).map_err(|e| format!("not an EAK provider export: {e}"))?;
        if cfg.format != EXPORT_FORMAT {
            return Err(format!(
                "not an EAK provider export (format {})",
                cfg.format
            ));
        }
        let mut checked = vec![];
        for p in &cfg.providers {
            let kind = catalog::provider(&p.provider)
                .map(|s| s.kind)
                .ok_or_else(|| format!("{}: unknown provider type {}", p.label, p.provider))?;
            let (label, spec, base_url) =
                normalise(&p.label, Some(p.provider.clone()), kind, p.base_url.clone())
                    .map_err(|e| format!("{}: {e}", p.label))?;
            checked.push((p, label, spec, base_url));
        }
        self.update(|s| {
            for (p, label, spec, base_url) in checked {
                let id = new_id(&s.providers);
                if cfg.default_provider.as_deref() == Some(p.id.as_str())
                    && s.default_provider.is_none()
                {
                    s.default_provider = Some(id.clone());
                }
                s.providers.push(ProviderProfile {
                    id,
                    label,
                    provider: spec.id.into(),
                    kind: spec.kind,
                    base_url,
                    model: clean(p.model.clone()),
                    org_id: clean(p.org_id.clone()).filter(|_| spec.org_project),
                    project_id: clean(p.project_id.clone()).filter(|_| spec.org_project),
                    models: vec![],
                    status: ConnectionStatus::default(),
                });
            }
            if s.default_provider.is_none() {
                s.default_provider = s.providers.first().map(|p| p.id.clone());
            }
            Ok(())
        })?;
        self.list()
    }
}

/// Tauri command wrappers: argument decoding only.
pub mod cmd {
    use super::*;
    use tauri::State;

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_catalog(state: State<'_, ModelSettings>) -> Vec<ProviderSpec> {
        state.catalog()
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_providers(state: State<'_, ModelSettings>) -> Result<Vec<ProviderView>, String> {
        state.list()
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_save_provider(
        state: State<'_, ModelSettings>,
        profile: ProfileInput,
        api_key: Option<String>,
    ) -> Result<ProviderView, String> {
        state.save(profile, api_key)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_remove_provider(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<(), String> {
        state.remove(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_set_default(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<Vec<ProviderView>, String> {
        state.set_default(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_set_key(
        state: State<'_, ModelSettings>,
        id: String,
        api_key: String,
    ) -> Result<ProviderView, String> {
        state.set_key(&id, &api_key)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_clear_key(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<ProviderView, String> {
        state.clear_key(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_list_models(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<Vec<ModelInfo>, String> {
        state.list_models(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_test_connection(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<ConnectionReport, String> {
        state.test_connection(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_export_config(state: State<'_, ModelSettings>) -> Result<String, String> {
        state.export_config()
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_import_config(
        state: State<'_, ModelSettings>,
        json: String,
    ) -> Result<Vec<ProviderView>, String> {
        state.import_config(&json)
    }
}
