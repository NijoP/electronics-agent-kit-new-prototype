//! Model settings: provider profiles, API keys in the OS credential store, model listing and
//! connection tests (Phase 4 "model adaptor").
//!
//! * Profiles (label, kind, base URL, model id) are plain JSON in the app config directory.
//!   They never contain a key.
//! * API keys live **only** in the operating-system credential store (Windows Credential
//!   Manager, macOS Keychain, Secret Service on Linux) through [`OsKeyStore`]. They are never
//!   written to a file, the IR, SQLite or a log, and never returned to the UI: the UI only
//!   learns whether a key exists (`has_key`).
//! * Listing models and testing a connection use `eak-intelligence` (`ureq` + rustls). Error
//!   text from the provider is shown, with the key redacted.
//!
//! Like the rest of this crate, everything here is a plain Rust function over
//! [`ModelSettings`] so it is tested headless; `cmd` holds the thin Tauri wrappers.

use eak_intelligence::message::CancelToken;
use eak_intelligence::provider::adaptor::{self, ConnectionReport};
use eak_intelligence::provider::http::{default_transport, HttpTransport};
use eak_intelligence::provider::{ApiKey, ProviderConfig, ProviderKind};
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

/// A saved provider. Holds no secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// What the UI sends to add or edit a provider (`id` empty or missing = new).
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileInput {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// What the UI sees: the profile plus key presence. Never the key.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub profile: ProviderProfile,
    pub has_key: bool,
    /// False for endpoints that normally run unauthenticated (Ollama, local servers).
    pub needs_key: bool,
    /// The credential store could not be read for this provider (shown, not hidden).
    pub key_error: Option<String>,
}

pub struct ModelSettings {
    file: Option<PathBuf>,
    profiles: Mutex<Vec<ProviderProfile>>,
    keys: Arc<dyn KeyStore>,
    transport: Arc<dyn HttpTransport>,
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn valid_base_url(u: &str) -> Result<(), String> {
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
        .ok_or("base URL must start with http:// or https://")?;
    if rest.is_empty() || rest.starts_with('/') || u.chars().any(char::is_whitespace) {
        return Err("base URL is not a valid address".into());
    }
    Ok(())
}

impl ModelSettings {
    /// `file` is the profile JSON (None = not persisted). A corrupt file is reported, never
    /// silently replaced.
    pub fn new(
        file: Option<PathBuf>,
        keys: Arc<dyn KeyStore>,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<ModelSettings, String> {
        let profiles = match &file {
            Some(f) if f.exists() => {
                let text = std::fs::read_to_string(f)
                    .map_err(|e| format!("cannot read {}: {e}", f.display()))?;
                serde_json::from_str(&text)
                    .map_err(|e| format!("{} is not valid model settings: {e}", f.display()))?
            }
            _ => Vec::new(),
        };
        Ok(ModelSettings {
            file,
            profiles: Mutex::new(profiles),
            keys,
            transport,
        })
    }

    /// Production wiring: OS credential store and the real HTTPS transport.
    pub fn system(file: Option<PathBuf>) -> Result<ModelSettings, String> {
        ModelSettings::new(file, Arc::new(OsKeyStore), default_transport())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<ProviderProfile>>, String> {
        self.profiles
            .lock()
            .map_err(|_| "model settings are poisoned; restart EAK".to_string())
    }

    fn persist(&self, profiles: &[ProviderProfile]) -> Result<(), String> {
        let Some(f) = &self.file else { return Ok(()) };
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let tmp = f.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, f).map_err(|e| format!("cannot replace {}: {e}", f.display()))
    }

    fn view(&self, p: &ProviderProfile) -> ProviderView {
        let (has_key, key_error) = match self.keys.get(&p.id) {
            Ok(k) => (k.is_some_and(|k| !k.trim().is_empty()), None),
            Err(e) => (false, Some(e)),
        };
        let cfg = config_for(p, None);
        ProviderView {
            profile: p.clone(),
            has_key,
            needs_key: cfg.requires_key(),
            key_error,
        }
    }

    pub fn list(&self) -> Result<Vec<ProviderView>, String> {
        let profiles = self.lock()?;
        Ok(profiles.iter().map(|p| self.view(p)).collect())
    }

    /// Add or update a provider; a non-empty `api_key` replaces the stored key.
    pub fn save(
        &self,
        input: ProfileInput,
        api_key: Option<String>,
    ) -> Result<ProviderView, String> {
        let label = input.label.trim().to_string();
        if label.is_empty() {
            return Err("give the provider a name".into());
        }
        let base_url = clean(input.base_url).map(|u| u.trim_end_matches('/').to_string());
        if let Some(u) = &base_url {
            valid_base_url(u)?;
        }
        let mut profiles = self.lock()?;
        let id = match clean(input.id) {
            Some(id) if profiles.iter().any(|p| p.id == id) => id,
            Some(id) => return Err(format!("unknown provider {id}")),
            None => new_id(&profiles),
        };
        let profile = ProviderProfile {
            id: id.clone(),
            label,
            kind: input.kind,
            base_url,
            model: clean(input.model),
        };
        if let Some(secret) = api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            self.keys.set(&id, secret)?;
        }
        let mut next = profiles.clone();
        match next.iter_mut().find(|p| p.id == id) {
            Some(slot) => *slot = profile.clone(),
            None => next.push(profile.clone()),
        }
        self.persist(&next)?;
        *profiles = next;
        Ok(self.view(&profile))
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut profiles = self.lock()?;
        if !profiles.iter().any(|p| p.id == id) {
            return Err(format!("unknown provider {id}"));
        }
        // Key first: if the store refuses, the profile stays so the user can retry.
        self.keys.delete(id)?;
        let next: Vec<_> = profiles.iter().filter(|p| p.id != id).cloned().collect();
        self.persist(&next)?;
        *profiles = next;
        Ok(())
    }

    pub fn set_key(&self, id: &str, api_key: &str) -> Result<ProviderView, String> {
        let profile = self.profile(id)?;
        let secret = api_key.trim();
        if secret.is_empty() {
            return Err("the key is empty; use remove key to delete it".into());
        }
        self.keys.set(id, secret)?;
        Ok(self.view(&profile))
    }

    pub fn clear_key(&self, id: &str) -> Result<ProviderView, String> {
        let profile = self.profile(id)?;
        self.keys.delete(id)?;
        Ok(self.view(&profile))
    }

    fn profile(&self, id: &str) -> Result<ProviderProfile, String> {
        self.lock()?
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

    /// Model ids the provider offers; the UI falls back to free text on `Err`.
    pub fn list_models(&self, id: &str) -> Result<Vec<String>, String> {
        let cfg = self.config(id)?;
        adaptor::list_models(&cfg, self.transport.clone(), &CancelToken::new())
            .map_err(|e| e.to_string())
    }

    /// A ready provider for the agent loop, or a plain reason why there is none (no
    /// profile, no model, or no key in the credential store).
    pub fn provider(&self, id: &str) -> Result<Box<dyn eak_intelligence::Provider>, String> {
        let cfg = self.config(id)?;
        eak_intelligence::provider::build_provider_with(&cfg, self.transport.clone())
            .map_err(|e| e.to_string())
    }

    /// One minimal real request. Always returns a report (failure is data, not an `Err`),
    /// except for an unknown provider id.
    pub fn test_connection(&self, id: &str) -> Result<ConnectionReport, String> {
        let cfg = self.config(id)?;
        Ok(adaptor::test_connection(
            &cfg,
            self.transport.clone(),
            &CancelToken::new(),
        ))
    }
}

/// Environment variable that is never set. Pointing `api_key_env` at it stops
/// `eak-intelligence` falling back to `OPENAI_API_KEY`/`ANTHROPIC_API_KEY`, so a key can only
/// come from the credential store.
const NO_ENV_KEY: &str = "EAK_KEYS_ARE_IN_THE_OS_CREDENTIAL_STORE_ONLY";

fn config_for(p: &ProviderProfile, key: Option<String>) -> ProviderConfig {
    let mut c = ProviderConfig::new(p.kind);
    c.base_url = p.base_url.clone();
    c.model = p.model.clone();
    c.api_key = key.as_deref().map(ApiKey::new);
    c.api_key_env = Some(NO_ENV_KEY.into());
    c.timeout_secs = 30;
    c
}

fn new_id(existing: &[ProviderProfile]) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut n = nanos;
    loop {
        let id = format!("prov-{n:x}");
        if !existing.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

/// Tauri command wrappers: argument decoding only.
pub mod cmd {
    use super::*;
    use tauri::State;

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
    ) -> Result<Vec<String>, String> {
        state.list_models(&id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn model_test_connection(
        state: State<'_, ModelSettings>,
        id: String,
    ) -> Result<ConnectionReport, String> {
        state.test_connection(&id)
    }
}
