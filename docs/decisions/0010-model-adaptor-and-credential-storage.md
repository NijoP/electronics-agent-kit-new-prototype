# ADR-0010 Model adaptor and credential storage

**Date:** 2026-11-14 · **Status:** accepted (product-owner directive 2026-11-13: outcome 4, universal model adaptor)

## Decision
* The desktop app lets the user add any number of model providers, enter an API key, pick a model,
  list the provider's models and run a connection test. Provider kinds are the existing
  `eak_intelligence::ProviderKind`s: Anthropic, OpenAI-compatible (user-supplied base URL, which
  covers OpenAI, OpenRouter, Groq, Together and local servers) and Ollama.
* API keys are stored **only** in the operating-system credential store through the `keyring` v3
  crate (features `apple-native`, `windows-native`, `sync-secret-service`, `crypto-rust`). They are
  never written to project files, the IR, SQLite, logs or error messages, and the UI never receives
  a stored key back (it sees `has_key`).
* Provider profiles (name, kind, base URL, model id) are plain JSON in the app config directory.
  They contain no secret.
* HTTP stays `ureq` + rustls, behind `eak-intelligence`'s `remote-http` feature, which the desktop
  crate enables. The transport gains a `get` method for model listings.
* `eak-intelligence` never reads `OPENAI_API_KEY`/`ANTHROPIC_API_KEY` for desktop providers: the
  desktop points `api_key_env` at a variable that is never set, so a key can only come from the
  credential store.

## Options considered
* Key in a config file or SQLite: rejected (owner requirement: OS credential storage only).
* Environment variables only: kept as the library default for headless use, disabled in the desktop.
* Vendor SDKs per provider: rejected; the three wire formats already exist in `eak-intelligence`.

## Consequences
* New direct dependency `keyring` (and its platform backends) in `apps/desktop/src-tauri`;
  recorded in `LICENSING.md`.
* On Linux the Secret Service (GNOME Keyring, KWallet) must be running; without it saving a key
  fails with a visible error and nothing is stored elsewhere.
* New IPC commands: `model_providers`, `model_save_provider`, `model_remove_provider`,
  `model_set_key`, `model_clear_key`, `model_list_models`, `model_test_connection`.
* Shared contract change: `HttpTransport` gains `get`; `ProviderKind` is now `Serialize`.

## Source requirement
Execution Instructions §5 ("model connections") and §12 ("user-selected providers"); product-owner
directive of 2026-11-13.
