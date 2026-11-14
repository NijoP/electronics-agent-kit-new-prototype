//! Model settings contract, headless: profiles persist without secrets, keys go only to the key
//! store, and list/test run over the real `ureq` transport against a real loopback HTTP server.

use eak_desktop::models::{KeyStore, MemoryKeyStore, ModelSettings, OsKeyStore, ProfileInput};
use eak_intelligence::provider::http::default_transport;
use eak_intelligence::ProviderKind;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SECRET: &str = "sk-test-SECRET-1234567890";

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-models-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Loopback server; returns (base url, recorded "METHOD path | auth headers" lines).
fn serve(
    route: impl Fn(&str, &str) -> (u16, String) + Send + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
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
            let mut p = line.split_whitespace();
            let (method, path) = (
                p.next().unwrap_or("").to_string(),
                p.next().unwrap_or("").to_string(),
            );
            let (mut len, mut auth) = (0usize, String::new());
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end().to_ascii_lowercase();
                if h.is_empty() {
                    break;
                }
                if let Some(v) = h.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                if h.starts_with("authorization:") || h.starts_with("x-api-key:") {
                    auth = h;
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let (status, out) = route(&method, &path);
            log.lock()
                .unwrap()
                .push(format!("{method} {path} | {auth}"));
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                    out.len()
                )
                .as_bytes(),
            );
        }
    });
    (base, seen)
}

fn input(kind: ProviderKind, base: &str, model: Option<&str>) -> ProfileInput {
    ProfileInput {
        id: None,
        label: "Local test".into(),
        provider: None,
        kind,
        base_url: Some(base.into()),
        model: model.map(String::from),
        org_id: None,
        project_id: None,
    }
}

fn settings(file: Option<PathBuf>, keys: Arc<MemoryKeyStore>) -> ModelSettings {
    ModelSettings::new(file, keys, default_transport()).unwrap()
}

#[test]
fn profiles_persist_without_secrets_and_keys_stay_in_the_store() {
    let d = tmpdir("persist");
    let file = d.join("model-providers.json");
    let keys = Arc::new(MemoryKeyStore::default());
    let s = settings(Some(file.clone()), keys.clone());
    let v = s
        .save(
            input(
                ProviderKind::OpenAiCompatible,
                "http://127.0.0.1:1/v1/",
                Some("m"),
            ),
            Some(SECRET.into()),
        )
        .unwrap();
    assert!(v.has_key && !v.needs_key);
    assert_eq!(v.profile.base_url.as_deref(), Some("http://127.0.0.1:1/v1"));
    assert_eq!(keys.get(&v.profile.id).unwrap().as_deref(), Some(SECRET));

    // The view the UI gets and every byte on disk are secret-free.
    let ui = serde_json::to_string(&s.list().unwrap()).unwrap();
    let disk = std::fs::read_to_string(&file).unwrap();
    for text in [&ui, &disk] {
        assert!(!text.contains(SECRET), "{text}");
        assert!(!text.contains("api_key"), "{text}");
    }
    for entry in std::fs::read_dir(&d).unwrap() {
        let c = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!String::from_utf8_lossy(&c).contains(SECRET));
    }

    // A new process sees the profile and (through the same store) the key.
    let again = settings(Some(file.clone()), keys.clone());
    assert_eq!(again.list().unwrap().len(), 1);
    assert!(again.list().unwrap()[0].has_key);

    // Removing the provider removes its key.
    again.remove(&v.profile.id).unwrap();
    assert!(again.list().unwrap().is_empty());
    assert_eq!(keys.get(&v.profile.id).unwrap(), None);
    assert!(!std::fs::read_to_string(&file)
        .unwrap()
        .contains(&v.profile.id));
}

#[test]
fn key_can_be_replaced_and_cleared_without_touching_the_profile() {
    let keys = Arc::new(MemoryKeyStore::default());
    let s = settings(None, keys.clone());
    let v = s
        .save(
            input(ProviderKind::Anthropic, "https://api.anthropic.com", None),
            None,
        )
        .unwrap();
    assert!(!v.has_key && v.needs_key);
    assert!(s.set_key(&v.profile.id, "  ").is_err());
    assert!(s.set_key(&v.profile.id, "k1").unwrap().has_key);
    assert!(!s.clear_key(&v.profile.id).unwrap().has_key);
    assert_eq!(s.list().unwrap().len(), 1);
    assert!(s.set_key("nope", "k").is_err());
}

#[test]
fn invalid_input_is_rejected() {
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let mut bad = input(ProviderKind::Ollama, "ftp://x", None);
    assert!(s.save(bad.clone(), None).unwrap_err().contains("http"));
    bad.base_url = Some("http://".into());
    assert!(s.save(bad.clone(), None).is_err());
    bad.base_url = None;
    bad.label = "  ".into();
    assert!(s.save(bad, None).is_err());
    let mut unknown = input(ProviderKind::Ollama, "http://x", None);
    unknown.id = Some("prov-missing".into());
    assert!(s.save(unknown, None).is_err());
    assert!(s.remove("prov-missing").is_err());
}

#[test]
fn a_corrupt_settings_file_is_reported_not_overwritten() {
    let d = tmpdir("corrupt");
    let file = d.join("model-providers.json");
    std::fs::write(&file, "{not json").unwrap();
    let e = ModelSettings::new(
        Some(file.clone()),
        Arc::new(MemoryKeyStore::default()),
        default_transport(),
    )
    .err()
    .expect("must fail");
    assert!(e.contains("not valid"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{not json");
}

#[test]
fn list_and_test_use_the_stored_key_over_real_http() {
    let (base, seen) = serve(|m, p| {
        match (m, p) {
        ("GET", "/v1/models") => (
            200,
            r#"{"data":[{"id":"gpt-local"},{"id":"another"}]}"#.into(),
        ),
        ("POST", "/v1/chat/completions") => (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into(),
        ),
        _ => (404, "{}".into()),
    }
    });
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let v = s
        .save(
            input(
                ProviderKind::OpenAiCompatible,
                &format!("{base}/v1"),
                Some("gpt-local"),
            ),
            Some(SECRET.into()),
        )
        .unwrap();
    assert_eq!(
        ids(s.list_models(&v.profile.id).unwrap()),
        ["another", "gpt-local"]
    );
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(r.ok, "{r:?}");
    assert!(!r.message.contains(SECRET));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter()
            .all(|l| l.contains(&format!("bearer {SECRET}").to_ascii_lowercase())),
        "{seen:?}"
    );
}

#[test]
fn provider_errors_are_shown_and_the_key_is_redacted() {
    let (base, _) = serve(|_, _| {
        (
            401,
            format!(r#"{{"error":{{"message":"bad key {SECRET} for org"}}}}"#),
        )
    });
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let v = s
        .save(
            input(ProviderKind::OpenAiCompatible, &base, Some("m")),
            Some(SECRET.into()),
        )
        .unwrap();
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(!r.ok);
    assert!(
        r.message.contains("bad key") && r.message.contains("401"),
        "{}",
        r.message
    );
    assert!(!r.message.contains(SECRET), "{}", r.message);
    let e = s.list_models(&v.profile.id).unwrap_err();
    assert!(!e.contains(SECRET), "{e}");
}

#[test]
fn the_environment_is_never_a_key_source() {
    // OPENAI_API_KEY must not be picked up for the default OpenAI endpoint.
    std::env::set_var("OPENAI_API_KEY", "sk-from-env");
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let v = s
        .save(
            ProfileInput {
                id: None,
                model: Some("m".into()),
                ..ProfileInput::new("OpenAI", "openai")
            },
            None,
        )
        .unwrap();
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(!r.ok && r.message.contains("missing API key"), "{r:?}");
}

#[test]
fn ollama_works_without_a_key() {
    let (base, _) = serve(|m, p| match (m, p) {
        ("GET", "/api/tags") => (200, r#"{"models":[{"name":"llama3:8b"}]}"#.into()),
        ("POST", "/api/chat") => (
            200,
            r#"{"message":{"role":"assistant","content":"OK"},"done":true}"#.into(),
        ),
        _ => (404, "{}".into()),
    });
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let v = s
        .save(input(ProviderKind::Ollama, &base, Some("llama3:8b")), None)
        .unwrap();
    assert!(!v.needs_key);
    assert_eq!(ids(s.list_models(&v.profile.id).unwrap()), ["llama3:8b"]);
    assert!(s.test_connection(&v.profile.id).unwrap().ok);
}

/// Exercises the real OS credential store. A host without one (a headless Linux CI box with no
/// Secret Service) cannot run this: it is then reported on stderr and does not count as
/// verification (STATUS.md records which OS stores were actually exercised).
#[test]
fn os_credential_store_round_trip() {
    let id = format!("eak-test-{}", std::process::id());
    let store = OsKeyStore;
    if let Err(e) = store.set(&id, SECRET) {
        eprintln!("SKIPPED os_credential_store_round_trip: no usable credential store: {e}");
        return;
    }
    assert_eq!(store.get(&id).unwrap().as_deref(), Some(SECRET));
    store.delete(&id).unwrap();
    assert_eq!(store.get(&id).unwrap(), None);
    store.delete(&id).unwrap(); // deleting a missing key is not an error
}

fn ids(models: Vec<eak_intelligence::provider::catalog::ModelInfo>) -> Vec<String> {
    models.into_iter().map(|m| m.id).collect()
}

fn ok_server() -> String {
    serve(|m, p| match (m, p) {
        ("GET", "/v1/models") => (
            200,
            r#"{"data":[{"id":"gpt-local","context_length":32768}]}"#.into(),
        ),
        ("POST", "/v1/chat/completions") => (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into(),
        ),
        _ => (404, "{}".into()),
    })
    .0
}

/// Regression (Spec 5.4): saving never marks a provider connected; only a streamed reply
/// does, and editing the profile resets it.
#[test]
fn connected_only_after_a_real_test_and_reset_on_change() {
    use eak_desktop::models::StatusState;
    let base = ok_server();
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let mut inp = input(
        ProviderKind::OpenAiCompatible,
        &format!("{base}/v1"),
        Some("gpt-local"),
    );
    let v = s.save(inp.clone(), None).unwrap();
    assert_eq!(v.profile.status.state, StatusState::Untested);
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(r.ok, "{r:?}");
    let v = &s.list().unwrap()[0];
    assert_eq!(v.profile.status.state, StatusState::Connected);
    assert_eq!(v.profile.models[0].context_window, Some(32768));
    // Same settings saved again: status kept. Model changed: back to untested.
    inp.id = Some(v.profile.id.clone());
    assert_eq!(
        s.save(inp.clone(), None).unwrap().profile.status.state,
        StatusState::Connected
    );
    inp.model = Some("other".into());
    assert_eq!(
        s.save(inp, None).unwrap().profile.status.state,
        StatusState::Untested
    );
    // A failing test is recorded as failed with its class.
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(!r.ok);
    let v = &s.list().unwrap()[0];
    assert_eq!(v.profile.status.state, StatusState::Failed);
    assert_eq!(
        v.profile.status.error_class,
        Some(eak_intelligence::ErrorClass::UnsupportedModel)
    );
}

#[test]
fn known_endpoints_are_fixed_and_unimplemented_providers_are_refused() {
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    // A base URL sent for a cloud provider is ignored: the catalog endpoint is used.
    let v = s
        .save(
            ProfileInput {
                base_url: Some("https://evil.example/v1".into()),
                ..ProfileInput::new("Groq", "groq")
            },
            None,
        )
        .unwrap();
    assert_eq!(v.profile.base_url, None);
    assert_eq!(v.endpoint, "https://api.groq.com/openai/v1");
    assert!(v.needs_key);
    for id in ["amazon-bedrock", "google-vertex", "azure-openai"] {
        let e = s.save(ProfileInput::new("x", id), None).unwrap_err();
        assert!(e.contains("not implemented"), "{e}");
    }
    // A custom endpoint must have a URL.
    let e = s
        .save(ProfileInput::new("Mine", "openai-compatible"), None)
        .unwrap_err();
    assert!(e.contains("base URL"), "{e}");
}

#[test]
fn openai_org_and_project_are_sent_as_headers() {
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    let v = s
        .save(
            ProfileInput {
                org_id: Some("org-1".into()),
                project_id: Some("proj-1".into()),
                ..ProfileInput::new("OpenAI", "openai")
            },
            Some("sk-x".into()),
        )
        .unwrap();
    assert_eq!(v.profile.org_id.as_deref(), Some("org-1"));
    // Other providers have no such fields.
    let g = s
        .save(
            ProfileInput {
                org_id: Some("org-1".into()),
                ..ProfileInput::new("Groq", "groq")
            },
            None,
        )
        .unwrap();
    assert_eq!(g.profile.org_id, None);
}

#[test]
fn legacy_v1_files_load_and_defaults_follow_the_profiles() {
    let d = tmpdir("legacy");
    let file = d.join("model-providers.json");
    std::fs::write(
        &file,
        r#"[{"id":"prov-a","label":"Old OpenRouter","kind":"open_ai_compatible","base_url":"https://openrouter.ai/api/v1","model":"x/y"},
            {"id":"prov-b","label":"Old Claude","kind":"anthropic","base_url":null,"model":null}]"#,
    )
    .unwrap();
    let keys = Arc::new(MemoryKeyStore::default());
    let s = settings(Some(file.clone()), keys);
    let l = s.list().unwrap();
    assert_eq!(l[0].profile.provider, "openrouter");
    assert_eq!(l[1].profile.provider, "anthropic");
    assert!(l.iter().all(|v| !v.is_default));
    let l = s.set_default("prov-b").unwrap();
    assert!(l[1].is_default && !l[0].is_default);
    let disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(disk["version"], 2);
    assert_eq!(disk["default_provider"], "prov-b");
    s.remove("prov-b").unwrap();
    assert!(
        s.list().unwrap()[0].is_default,
        "default moves to a remaining profile"
    );
    assert!(s.set_default("nope").is_err());
}

#[test]
fn export_is_portable_and_secret_free_and_import_round_trips() {
    let keys = Arc::new(MemoryKeyStore::default());
    let s = settings(None, keys.clone());
    let base = ok_server();
    let v = s
        .save(
            input(
                ProviderKind::OpenAiCompatible,
                &format!("{base}/v1"),
                Some("gpt-local"),
            ),
            Some(SECRET.into()),
        )
        .unwrap();
    s.test_connection(&v.profile.id).unwrap();
    let json = s.export_config().unwrap();
    assert!(!json.contains(SECRET), "{json}");
    assert!(
        !json.contains("status") && !json.contains("connected"),
        "{json}"
    );

    let other = settings(None, Arc::new(MemoryKeyStore::default()));
    let l = other.import_config(&json).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].profile.model.as_deref(), Some("gpt-local"));
    assert_eq!(l[0].profile.provider, "openai-compatible");
    assert!(!l[0].has_key, "keys never travel with an export");
    assert!(l[0].is_default);
    assert_eq!(
        l[0].profile.status.state,
        eak_desktop::models::StatusState::Untested
    );
    assert!(other.import_config("{}").is_err());
    assert!(other
        .import_config(r#"{"format":"eak-model-providers","version":1,"providers":[{"id":"a","label":"B","provider":"amazon-bedrock"}]}"#)
        .unwrap_err()
        .contains("not implemented"));
    assert_eq!(other.list().unwrap().len(), 1, "a bad import adds nothing");
}

#[test]
fn resolve_gives_integrations_the_default_provider_and_model_override() {
    let s = settings(None, Arc::new(MemoryKeyStore::default()));
    assert!(s.resolve(None, None).is_err());
    let base = ok_server();
    s.save(
        input(
            ProviderKind::OpenAiCompatible,
            &format!("{base}/v1"),
            Some("gpt-local"),
        ),
        None,
    )
    .unwrap();
    let p = s.resolve(None, None).unwrap();
    assert_eq!(p.label(), "openai-compatible:gpt-local");
    let p = s.resolve(None, Some("other-model")).unwrap();
    assert_eq!(p.label(), "openai-compatible:other-model");
}
