# EAK development status

Truthful record of what exists and what has been verified. "Implemented" means code exists and
the listed tests pass; it does not mean professionally validated.

## Model adaptor: providers, OS-keyring keys, model list, connection test (2026-11-14)

**Platform tested:** Linux (Ubuntu, GNOME Secret Service running), Rust stable. Windows and macOS credential stores: **not run** (the code uses `keyring`'s `windows-native`/`apple-native` backends; unverified there). No real provider account was used.

### Implemented and tested
- `eak-intelligence`: `HttpTransport::get`; `provider::adaptor::{list_models, test_connection}` for Anthropic, OpenAI-compatible (any base URL: OpenAI, OpenRouter, Groq, Together, local servers) and Ollama. Provider error text is returned with the key redacted. Unit tests (canned JSON) plus `tests/adaptor_http.rs`: the real `ureq` transport against a real loopback HTTP server (list, test, 401 text, unreachable endpoint).
- `apps/desktop/src-tauri/src/models.rs`: profile store (JSON, no secrets), `KeyStore` with `OsKeyStore` (keyring v3), IPC commands `model_*` (7). The environment is never a key source in the desktop. `tests/models.rs` (9 tests): profiles/UI view/disk contain no key, remove deletes the key, corrupt file is not overwritten, list/test over real HTTP using the stored key, redaction, env not used, Ollama keyless, and a **real OS credential store round trip** (ran and passed on Linux).
- UI: `ModelSettings.tsx` (add/edit/remove provider, presets, write-only key field, fetch models into a picker with free-text fallback, test connection), opened from a button in `App.tsx`. 5 vitest tests added.

### Not verified / not implemented
- Never run against a live provider endpoint (no key was available); wire formats are checked against canned and loopback responses only.
- Windows Credential Manager and macOS Keychain paths unexercised. If no Secret Service runs on Linux, saving a key fails visibly.
- Not exercised in a real window. The settings button is a floating button, not part of the toolbar.
- The agent loop is not yet wired to the chosen provider in the UI (this task is the adaptor and settings only).

## Desktop shell: Tauri 2 + React bound to the command engine (2026-11-13)

**Platform tested:** Windows 11, Rust 1.99.1 `x86_64-pc-windows-gnu`, Node 24.21 / npm 11.19.
**CI (commit 599bf91, pull-request run https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/43431886918):**
all 8 jobs passed: `rust` (policy script, fmt, clippy, `cargo test --workspace --release`) and `desktop` (`npm ci`, typecheck, vitest, `tauri build --no-bundle`) on `windows-latest`, `ubuntu-latest`, `macos-latest` (arm64) and `macos-15-intel` (x64). Linux and macOS are verified by CI only, not on a local machine; no window was opened on any runner.

### Implemented and tested
- `apps/desktop/src-tauri` (crate `eak-desktop`, a workspace member) holds the IPC layer, which is plain Rust functions over one `Session` (one open `Engine` plus the SQLite recent-project index):
  - create, open (with recovery and explicit stale-lock take-over) and close a project;
  - `execute_command`, which always uses the **Human** actor with the revision the UI rendered;
  - undo/redo, `events_since` and recovery acknowledgement;
  - ERC/DRC via `eak-validation`, with the report saved as revision-bound evidence;
  - recent projects.
  - Geometry for the views (absolute pins and pads) is derived on each request through `eak-core`. The UI keeps no project model.
- The React/TypeScript/Vite UI has these parts:
  - Project Home: recent projects, open and create.
  - Workspace: a toolbar, a command palette (Ctrl/Cmd+K, keyboard navigation, confirmation for destructive commands) and keyboard undo/redo.
  - Read-only SVG schematic and PCB views with wheel zoom and drag pan. Selecting an item cross-probes by `symbol_id` in both directions and from findings.
  - Engineering Monitor: ERC/DRC findings, outcome text, a stale-revision marker, the not-implemented checks and the limitations.

### Tests executed (exact results, Windows)
- `cargo test -p eak-desktop --release`: **8 passed, 0 failed**. Covered:
  - Human-actor execute, undo and redo, with stale-revision rejection;
  - events;
  - reopen with rebuilt history and the recent index;
  - invalid paths;
  - lock take-over;
  - unknown-outcome write block and its acknowledgement;
  - ERC/DRC revision binding and evidence;
  - geometry identical to `Project::pin_position`/`pad_position` and cross-probe ids;
  - frontend and Rust IPC command lists match.
- `npm test` (vitest, jsdom): **18 passed, 0 failed**.
- `npm run typecheck`: clean. `npm run build` (Vite): succeeds.
- `cargo test --workspace --release`: **43 passed, 0 failed** (M1 suite 35 + eak-desktop 8).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`: clean.
- `npm run tauri -- build --no-bundle`: succeeds and produces `eak-desktop.exe` with the embedded frontend. A 6-second launch smoke check stayed running and created the SQLite index in the app data folder; no interactive use.

### Not verified / not implemented
- The app has not been exercised interactively in a real WebView in this change. Rendering, focus and layout are covered only by jsdom tests.
- No editing tools, 3D viewport, library browser, Terminal Center, Manufacturing Center or Agent Studio yet.
- No dockable, resizable or persisted panels (UI-01 is PARTIAL).
- Project folders are typed as absolute paths; there is no native folder picker.
- Installers are not produced (`bundle.active: false`).

## Milestone M1: engineering foundation and headless vertical slice (2026-11-13)

**Platform tested:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`, WinLibs GCC 16.2,
AMD Ryzen 7 7840HS, 15.3 GB RAM. Linux and macOS: not run at M1; the first CI run is recorded under "Desktop shell" above.

### Implemented and tested
- EAK-IR v1 (`eak-core`): project, library, schematic, PCB, constraints, manufacturing config;
  integer-nanometre geometry; byte-stable JSON; schema version gate (newer/corrupt files are
  rejected safely); integrity checker; 7 built-in generic components.
- Engineering core (`eak-engineering`):
  - IPC-2221 trace width, clearance, creepage and via current, re-implemented from the DESK
    audit, with DESK reference vectors as tests (tables **not verified against the standard text**).
  - Circuit calculations.
  - Schematic netlist resolution.
  - Exact copper geometry and connectivity, with zone fill on a 0.1 mm raster.
  - A native two-layer A* router.
  - A constructive placer.
  - 3D scene data.
- Verification (`eak-validation`):
  - ERC: 13 rules.
  - DRC: 13 rules, including current capacity and voltage clearance.
  - Mechanical: 3 rules.
  - Every report is bound to a revision and a SHA-256 input hash, and lists the checks that are not implemented.
- Runtime (`eak-runtime`):
  - Typed command set (38 commands).
  - Revision checks and the agent approval policy.
  - Transactional apply with integrity gate, diff-based undo/redo and an fsync'd journal with crash reconciliation (UNKNOWN_OUTCOME blocks writes).
  - Checkpoints, project lock, evidence store.
  - Project-scoped process supervisor (timeout, cancel, bounded output).
  - Task model and SQLite index.
- Manufacturing (`eak-manufacturing`):
  - Outputs: RS-274X Gerber (9 layers), Excellon, BOM, CPL.
  - SHA-256 manifest.
  - Release gate that requires revision-bound ERC/DRC/MECH with zero errors, otherwise the package is marked DRAFT.
  - Package re-verification that detects tampering.

### Tests executed (exact results)
`cargo test --workspace --release`: **35 passed, 0 failed** (eak-core 7, eak-engineering 11,
eak-runtime 10 engine + 4 process, eak-manufacturing 3 end-to-end).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.

The key end-to-end test (`reference_design_end_to_end`) runs a 10-component ATtiny85 + AMS1117 board through
the command API:
1. Schematic, then ERC: 0 errors.
2. Sync, place, auto-route (**20/20 connections**) and a GND pour.
3. DRC: 0 errors; MECH: 0 errors.
4. Package status: InternallyVerifiedReleaseCandidate; the package re-verifies and tampering is detected.
Injected short and clearance violations are detected (`drc_detects_injected_short_and_clearance_violation`).

### Not yet implemented (acceptance gaps)
- Desktop application: superseded. See "Desktop shell" above (read-only views; editors not started).
- Intelligence Engine: core crate exists (see below); no UI, no live-provider run, no context engine/skills.
- Symbol/footprint editors, 3D viewport UI, Terminal Center UI, research browser, LCSC/JLCPCB integrations.
- Multi-layer (>2) boards, differential pairs, impedance, thermal reliefs, silkscreen text.
- Linux/macOS builds have not been executed yet; packaging and installers do not exist yet.
- Independent engineering review and fabricator DFM: not performed (required before any manufacturing claim).

## Intelligence Engine core: `eak-intelligence` (2026-11-13)

**Platform tested:** same host as M1 (Windows 11, Rust 1.99.1 `x86_64-pc-windows-gnu`). Linux and macOS: CI only.

### Implemented and tested
- **Providers:**
  - A `Provider` trait with OpenAI-compatible, Anthropic Messages and Ollama adapters.
  - Cancellable HTTPS transport (ureq + rustls) behind the optional `remote-http` Cargo feature (off by default): bounded retries on 429, 5xx and transport errors; typed error mapping; 16 MiB body limit. Without the feature, remote providers fail with `ProviderError::TransportNotCompiledIn` and nothing touches the network.
  - Model and endpoint come from the user's `ProviderConfig`.
  - Keys come from the configuration value or an environment variable (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY` by default) and are never serialised or printed.
  - Anthropic assistant content, including thinking blocks, is replayed verbatim.
- **Tools:**
  - One typed tool per `Command` variant (38).
  - Arguments decode only through serde into the real `Command`; an exhaustive `match` forces a schema for every new variant.
  - Read-only tools: design summary, library, netlist, ERC, DRC, and calculators. The calculators are trace width, conductor clearance, LED resistor, divider, linear regulator and RC; they call `eak-engineering`.
- **Agent loop:**
  - Bounded turns, tool calls per turn and tool-output size.
  - Cancellation is checked before every provider and tool call and aborts in-flight HTTP. The transcript stays valid after a cancel.
  - Modes: Manual, Assisted and Autonomous.
    - Manual previews and records a plan; nothing is applied until `execute_plan`.
    - Assisted asks for per-command approval.
    - Autonomous applies commands within the user's pre-authorised scope; destructive commands always ask.
  - Every mutation is previewed on a sandbox copy, then submitted as an `Agent`-actor `CommandRequest` carrying an approval that covers its class.
  - Stale views and stale plans are refused.
  - The `AgentRun` log records actions, approvals, responses and events.

### Tests executed
`cargo test --workspace --release`: **69 passed, 0 failed** (the 35 M1 tests plus 34 new). Clippy (`-D warnings`), `cargo fmt --check` and the Python-free scan are clean.

`cargo test -p eak-intelligence`: **34 passed, 0 failed** with and without `--features remote-http` (CI runs both, plus `--no-default-features`):
- 21 unit tests: providers, transport (including the typed `TransportNotCompiledIn` refusal without `remote-http`), key handling, schemas against the serde shape, read-only tools.
- 13 integration tests in `tests/agent.rs`, run with a scripted provider and no network.

### Known gaps
- No run against a live model endpoint, and no streaming.
- No approval/Agent Studio UI; the run log is not yet written to `evidence/`.
- Not exposed as tools yet: manufacturing export (`eak_manufacturing::compile`), MECH, via-current and creepage calculators. Project create/open stays a host action.
- Manual/Assisted/Autonomous are not defined in the source-of-truth PDFs; the mapping is recorded in `docs/REQUIREMENTS_TRACEABILITY.md` (INT-07).

## Coordination note
The Firstmate crew could not be started on this host: the Firstmate session lock does not
acquire under native Windows Git Bash (upstream fix pending), and no WSL distribution is
installed. Development is proceeding directly in this repository.

## Next dependency-ready work
1. Interactive schematic editor driving the same commands (the desktop shell above is the base).
2. PCB editor and 3D view; Manufacturing Center and installer bundles.
3. Intelligence Engine: manufacturing-export and MECH tools, run-log evidence persistence, desktop approval UI.
