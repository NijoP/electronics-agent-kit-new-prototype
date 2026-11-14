# Security notes

Prototype software; no security review has been done. Report issues privately to the maintainers.

- **Credentials:** model API keys live in the OS credential store only, never in project or profile files. External agent CLIs keep their own authentication; EAK does not read it.
- **Design writes:** every mutation is a typed `Command` through `Engine::execute` with a revision check, an approval policy (agent requests need an approval covering the risk class) and a journal naming the actor (`agent:<name>@<session>` for harness calls). Destructive harness commands need an explicit opt-in (`EAK_HARNESS_ALLOW_DESTRUCTIVE=1`). Code-reviewed only for the harness.
- **App endpoint:** the harness endpoint is token-authenticated and local only (Unix socket; loopback TCP on Windows), advertised in `<project>/.harness.json`; calls for a project the app does not have open are refused. Code-reviewed only; no penetration test.
- **Terminal control API:** token-authenticated loopback JSON API (`docs/terminal-api.md`). Terminal sessions are real OS processes with the user's privileges; closing a session signals only processes EAK spawned.
- **Network:** model providers, LCSC lookup (read-only) over HTTPS with rustls. Nothing is uploaded or ordered.
- **Installers:** unsigned; SHA-256 verified against the release `SHA256SUMS` (`docs/INSTALL.md`).
- Never commit secrets, copyrighted datasheets or API keys.

## Targeted review: harness, terminal, providers (2026-11-15, code reading only)
Static review for the integration directive §6; no penetration test, nothing run.

Checked and adequate as written:
- **Keys:** model API keys are read from the OS credential store (`keyring` v3, `src-tauri/src/models.rs`); the app points `api_key_env` at a never-set variable so `OPENAI_API_KEY`/`ANTHROPIC_API_KEY` are not picked up silently; `ApiKey` is never serialised, its `Debug` is redacted and provider errors are redacted (`eak-intelligence/src/provider/adaptor.rs`). No key is put into terminal session environments.
- **Harness endpoint:** 128-bit random token in a 0600 file in a 0700 state dir, Unix socket 0600, constant-time token comparison, request size limit (`eak-harness/src/endpoint.rs`). Terminal sessions receive the token *file path* (`EAK_HARNESS_TOKEN_FILE`), not the token.
- **Terminal control API:** loopback TCP, token in a 0600 file, constant-time comparison (`eak-terminal/src/api.rs`).
- **Harness call log** (`harness-calls.jsonl`) records tool, actor, outcome and revisions only, not arguments.
- **Rejected from DESK:** its file-based credential store uses reversible base64/XOR obfuscation (`desk/auth/storage.py`); EAK does not inherit it.

Fixed:
- Per-session terminal environment (`env` passed to `session.create`, which can carry credentials) was written in plain text into the terminal layout file; it is now stripped before saving (`eak-terminal/src/lib.rs`, regression test `session_environment_is_not_persisted`, unrun).

Unresolved risks:
- **Windows:** the token files and the harness/terminal state folders get no ACL hardening (`cfg(unix)` only); any process of another user with read access to the profile could read them. Loopback TCP is reachable by every local process that has the token.
- **Same-user processes:** anything running as the user (including every agent in an EAK terminal, by design) can read the harness token file and drive design commands within the session approval policy; destructive commands still need `EAK_HARNESS_ALLOW_DESTRUCTIVE=1`.
- **Environment inheritance:** terminal sessions inherit the desktop app's full environment. If the user launched EAK from a shell that exports API keys, every session (and agent) sees them. This is intended (agents use their own native auth) but undocumented in the UI.
- **Terminal scrollback** is kept in memory; anything printed (including secrets) is visible to the terminal API holder. The terminal layout file is written with default permissions (it no longer contains env, but does contain commands and paths).
- **File access:** harness `project_dir` / `create_project` accept any absolute path the user can write; there is no project-root allowlist.
- **Redaction is exact-match:** a provider error echoing a transformed key (e.g. URL-encoded or truncated) would not be redacted.
- **Dependencies:** no `cargo audit`/`cargo deny` run in this review (testing freeze).
