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
        kind,
        base_url: Some(base.into()),
        model: model.map(String::from),
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
    let (base, seen) = serve(|m, p| match (m, p) {
        ("GET", "/v1/models") => (
            200,
            r#"{"data":[{"id":"gpt-local"},{"id":"another"}]}"#.into(),
        ),
        ("POST", "/v1/chat/completions") => (
            200,
            r#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#.into(),
        ),
        _ => (404, "{}".into()),
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
        s.list_models(&v.profile.id).unwrap(),
        ["another", "gpt-local"]
    );
    let r = s.test_connection(&v.profile.id).unwrap();
    assert!(r.ok, "{r:?}");
    assert!(!r.message.contains(SECRET));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
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
                label: "OpenAI".into(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: None,
                model: Some("m".into()),
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
    assert_eq!(s.list_models(&v.profile.id).unwrap(), ["llama3:8b"]);
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
