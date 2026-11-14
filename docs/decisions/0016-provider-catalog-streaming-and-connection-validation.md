# ADR-0016 Provider catalog, streaming and staged connection validation

**Date:** 2026-11-15 · **Status:** accepted (product-owner IDE upgrade directive §5, §8; direction
correction of 2026-11-14: providers are optional infrastructure, not an assistant experience).
Extends ADR-0010 (model adaptor and credential storage).

## Context
ADR-0010 gave EAK three wire formats (OpenAI-compatible, Anthropic, Ollama), keys in the OS
credential store and a one-shot connection test. Provider "types" existed only as a preset list in
the renderer, so every provider was effectively "a base URL": known endpoints were typed by hand,
there was no Gemini, no model metadata, no streaming, no default provider, and failures were not
classified (a wrong model, a wrong URL and a dead server all looked like "HTTP 404" or a transport
string). A saved model + key counted as ready without any test. The owner then corrected the
product direction: AI is optional; agents run as their own CLIs in the terminal with their own
sign-in; EAK's provider layer is infrastructure for EAK-native integrations, configured only in
Settings.

## Decision
* **Catalog in the backend** (`eak_intelligence::provider::catalog`), after Pi's provider registry
  (`packages/ai`, MIT; studied, no code copied): each provider is an id, a protocol family
  (`ProviderKind`), a fixed endpoint (cloud), an editable default (local) or none (custom), an auth
  method, OpenAI org/project support, discovery support, and `implemented` + a reason when not.
  Implemented: OpenAI, Anthropic, Google Gemini, OpenRouter, Mistral, Groq, xAI, DeepSeek, Cerebras,
  Together, Fireworks, NVIDIA NIM, Ollama, LM Studio, custom OpenAI-compatible, custom
  Anthropic-compatible. Listed as **not implemented**: Google Vertex AI (ADC/service accounts),
  Amazon Bedrock (SigV4), Azure OpenAI (deployment URLs/api-key header). Saving one is refused.
* **New protocol family `Gemini`** (`generateContent` / `streamGenerateContent?alt=sse`, key in the
  `x-goog-api-key` header, model parts replayed verbatim so `thoughtSignature`s survive).
* **Streaming**: `Provider::stream(request, cancel, sink)` with a default that wraps `complete`;
  SSE adapters for OpenAI-compatible, Anthropic and Gemini, NDJSON for Ollama. `HttpTransport`
  gains `post_stream` (the default reads the whole body; the ureq transport delivers lines as they
  arrive, with cancel and idle timeout). OpenAI-compatible quirks are detected from the base URL
  (Pi's `detectCompat` idea): `max_completion_tokens` for api.openai.com, no `stream_options` for
  Mistral.
* **Error classes** (`ErrorClass`): missing API key, invalid API key, authentication failed,
  unsupported model, invalid base URL, network error, timeout, rate limited, provider unavailable,
  request rejected, invalid response, invalid configuration, not implemented, cancelled.
* **Staged connection test**: configuration → authentication via model listing → model offered →
  one real streamed request. `connected` only after a streamed reply; listing alone gives
  `authenticated`; any change to endpoint, model or key resets the stored status to `untested`.
* **Desktop settings file v2** `{version, default_provider, providers}`; v1 (bare array) still
  loads and catalog ids are inferred. Profiles gain catalog id, org/project ids, discovered models
  and last status. Portable export/import without keys or status.
* **Runtime API** `ModelSettings::resolve(profile?, model?)` returns a ready provider (the default
  profile when none is named) for future EAK-native integrations. No assistant/chat UI is added.
* EAK never reads, copies or modifies external CLI credentials or config (`~/.pi`, opencode,
  claude, …).

## Options considered
* Port Pi's TypeScript `packages/ai` or embed it via Node: rejected (Rust backend, no Node runtime
  dependency, and the renderer must never hold keys).
* Vendor SDKs per provider: rejected; four wire formats cover the catalog.
* Keep presets in the renderer: rejected; the backend must validate endpoints and refuse
  unimplemented providers.

## Consequences
* Shared-contract changes: `ProviderKind::Gemini`; `ProviderError::{InvalidBaseUrl,
  NotImplemented}`; `Provider::stream` (defaulted); `HttpTransport::post_stream` (defaulted);
  `ProviderConfig::{extra_headers, key_required}` (serde-defaulted); `StreamEvent`;
  `adaptor::list_models` returns `ModelInfo` instead of `String`; `ConnectionReport` gains
  `state`, `error_class`, `stages`, `models`. New IPC: `model_catalog`, `model_set_default`,
  `model_export_config`, `model_import_config`.
* No new dependencies.
* Live provider invocation is unverified (no paid keys; Ollama not installed on the build host).
  Wire formats are checked against recorded-shape fixtures and a loopback server, including
  incremental chunked streaming over a real socket.

## Source requirement
EAK IDE upgrade directive §5.1–5.6, §8, §10 (provider tests); direction correction 2026-11-14.
