# EAK work backlogs (all Firstmate homes, frozen 2026-11-13 ~21:00 IST)

Copied verbatim from each home's backlog. Item ids match branch names `fm/<id>` where a branch exists. Status "In flight" means a worker had started; all workers were stopped for the Linux move, and all of their committed work is on GitHub (see LINUX_HANDOFF.md).

---

# Home: main firstmate

## Backlog

## In flight
- [ ] eak-scope-audit - EAK general-purpose scope + frozen-stack audit, DESK study, domain-pack design, gap analysis (repo: eak) (kind: scout) (since 2026-11-13)
  Owner directives 2026-11-13: product scope correction (general-purpose platform, domain packs, RF first-class) and frozen-stack audit. Supersedes eak-stack-audit (folded in).
- [ ] eak-val-power - EAK acceptance project B: power-electronics PCB designed in EAK (repo: eak) (kind: ship) (since 2026-11-13)
  Mode no-mistakes, yolo on. Deadline 22:39 IST. Owns validation/power-buck/ only; gaps route to owning secondmate via firstmate. Independent verification is a separate later scout.
- [ ] eak-val-esp32 - EAK acceptance project A: ESP32 dashboard designed in EAK (repo: eak) (kind: ship) (since 2026-11-13)
  Mode no-mistakes (mixed/product-facing validation), yolo on (owner delegated EAK merges 2026-11-13). Deadline 22:39 IST. Owns validation/esp32-dashboard/ only; gaps route to owning secondmate via firstmate. Independent verification is a separate later scout.
- [ ] eak-dev-method - EAK AGENTS.md: full development-method section (Ponytail-derived) (repo: eak) (kind: ship) (since 2026-11-13)
  Mode direct-PR (contributor/process docs on no-mistakes-prod-only project), yolo off. Owner directive 2026-11-13: adopt Ponytail (MIT, github.com/DietrichGebert/ponytail) minimal-code methodology in EAK's project agent instructions; paraphrase, cite provenance, copy no code.
## Queued
- [ ] eak-stack-audit - EAK frozen-stack compliance audit (baseline matrix) (repo: eak) (kind: scout) (since 2026-11-13)
  Owner directive 2026-11-13: audit repo against the frozen stack before implementation; baseline for the final stack-compliance matrix. Re-run near the deadline.
## Done

---

# Home: eak-core

## In flight
- [ ] eak-scripts-exec - Set the executable bit on scripts/*.sh so Linux/macOS CI passes (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-core. Every EAK PR is red on ubuntu/macos at the Python-free policy step because scripts/*.sh were committed from Windows without +x. Land the mode change alone, first, to unblock all PRs. Delivery: direct-PR (internal CI tooling only), yolo on.
- [ ] eak-cli-ci - EAK CLI over the command engine and green cross-platform CI (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-core. Create apps/cli (binary 'eak'): new/open project, apply a JSON command file through Engine::execute, undo/redo, run ERC/DRC/MECH reports, compile a manufacturing package, list checkpoints; JSON output mode for automation. Add it to the workspace members. Get the CI matrix (windows-latest, ubuntu-latest, macos-latest, macos-15-intel) green and fix platform-specific failures, especially process.rs Unix process-group kill tests. Integration tests drive the CLI end to end on the reference design. Update STATUS.md, CHANGELOG.md and docs/REQUIREMENTS_TRACEABILITY.md truthfully in the same PR. Delivery: direct-PR: no-mistakes review broken on Windows (main firstmate decision 2026-11-13); 4-job CI matrix is the mandatory merge gate plus firstmate diff review; yolo on. libc (cfg(unix)) dependency approved by the owner 2026-11-13.
## Queued
- [ ] eak-release-pkg - Cross-platform release packaging and installers for EAK blocked-by: eak-cli-ci (repo: eak) (kind: ship) (priority: 1) (since 2026-11-13)
  Owner: eak-core. Toward a complete installable cross-platform product: a release workflow that builds and packages the eak CLI for Windows, Linux and macOS, plus installer/bundle integration for the desktop app coordinated with the desktop domain (it owns the Tauri app). Depends on eak-cli-ci. Delivery: direct-PR: no-mistakes review broken on Windows; yolo on. Briefs must carry the frozen stack, generic-platform rule (no project-specific code), resource limits (CARGO_BUILD_JOBS=2, max 2 workers), and the development method.
- [ ] eak-session-api - Propose the minimal Rust session/agent supervisor interface for the Terminal Center blocked-by: eak-cli-ci (repo: eak) (kind: scout) (since 2026-11-13) (hold: fm-hold-v1:UGFya2VkIGJ5IHRoZSAyMDI2LTExLTE0IGZpbmFsIGRpcmVjdGl2ZTogVGVybWluYWwgQ2VudGVyIC8gYWdlbnQtbWFuYWdlbWVudCB0b29saW5nIGlzIG9mZiB0aGUgY3JpdGljYWwgcGF0aA==) (hold-kind: owner)
  Owner hold set: 2026-11-13T15:19:47Z

  Owner: eak-core. Owner requirement (2026-11-13): eak-core owns Rust process supervision, session lifecycle and typed command interfaces for sessions/agents behind the Advanced/Developer Terminal Center (named, persistent, project-scoped terminal sessions; concurrent supervised AI agents each with project/task/model/session/ownership identity; live stdout/stderr; create/switch/rename/terminate; cancellation, timeouts, recovery after interruption; command history; cwd/project context; resource monitoring; safe child-process cleanup; one project failure never kills another). Before building, send the main firstmate the minimal Rust interface eak-core will expose (session/agent supervisor API) so eak-desktop and eak-ux build against an agreed contract. Herdr is concept reference only (check licence; re-implement, no dependency, no copied code). Any new dependency (e.g. PTY crate) needs written justification for owner approval. Sequenced behind landing eak-cli-ci; respect the 2-worker memory cap.
- [ ] eak-reliability - EAK CI reliability program: no-mistakes-style root-cause fixes and recurring-failure register (repo: eak) (kind: ship) (priority: 2) (since 2026-11-13)
  Owner: eak-core. Owner request (verbatim intent): use and study https://github.com/kunchenguid/no-mistakes as a reference for improving EAK development reliability and eliminating recurring mistakes. Audit the current EAK repository, GitHub Actions workflows, pull requests, failed status checks, CI configuration, build errors, test failures and integration issues; apply the relevant no-mistakes methodology to prevent repeated failures. Create a systematic process to: identify and fix every outstanding failed or pending check; diagnose root causes instead of superficial fixes; prevent regressions through automated tests and CI; verify every PR before merging; keep Windows, Linux and macOS CI healthy; maintain a record of recurring failures and their permanent fixes; ensure Firstmate review, merge and supervision workflows work reliably on Windows. Do not blindly copy the repository or introduce unnecessary dependencies; preserve the approved Rust, Tauri 2, React and TypeScript stack, the Python-free requirement and all source-of-truth requirements. Expectation: the EAK GitHub repository stays clean, every required check has a verified result, and the same mistakes do not keep recurring. Known state at intake: open PRs #1 #4 #5 #6 #7 #8 #10 are red only because they predate the scripts executable-bit fix (PR #2) and need rebasing; #3 #9 #11 #12 are green; the macos-13 runner was replaced by macos-15-intel in PR #2. Deliverables: a recurring-failures register in the repo (docs/RELIABILITY.md: symptom, root cause, permanent fix, guard test/CI check), CI hardening for the recorded failure classes (for example a check that tracked scripts keep their executable bit, required checks configured consistently), and a short PR-verification checklist in AGENTS.md; coordinate rebases of other domains PRs through the main firstmate rather than editing them.
- [ ] eak-editor-cmds - Editor-enabling typed commands in eak-runtime + command reference doc (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Routed by main firstmate 2026-11-13 15:17Z (final directive, deadline 2026-11-14 07:00 IST). Ensure eak-runtime exposes every typed command the schematic and PCB editors need (place/move/rotate/delete symbol; add/edit/delete wire with explicit junctions; net labels/power symbols; properties; sheets; board outline; footprint place/rotate/lock; track add/edit/delete; via; zone; rules; save/reopen; revision list) with transactions + undo/redo, and EAK-IR connectivity that never infers a connection from a visual crossing; publish a short command reference doc for eak-desktop and eak-intelligence. Delivery: direct-PR, yolo on (no-mistakes review broken on Windows; 4-job CI matrix is the merge gate). Do not edit PadDef/PadKind (eak-electronics owns).
## Done

---

# Home: eak-dashboard

## Backlog

## In flight
- [ ] devdash-mvp - EAK dev control center: first working dashboard in tools/devdash (repo: eak) (kind: ship) (since 2026-11-13)
  Routed by main firstmate (corr=567ade7aacf49b40). mode=direct-PR, yolo=on. direct-PR: no-mistakes review broken on Windows; also internal-only developer tooling. Merge gate: full CI matrix green plus own diff review of dependencies and stack. Waiting on a pool slot this clone can lease.
## Queued
## Done

---

# Home: eak-desktop

## In flight
- [ ] eak-desktop-shell - Tauri 2 desktop shell bound to the command engine (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-desktop. Create apps/desktop: Tauri 2 Rust shell (src-tauri, added to workspace members - that one-line Cargo.toml member edit is permitted) plus React/TypeScript/Vite UI. Tauri commands wrap eak-runtime Engine (open/create project, execute CommandRequest as Human actor, undo/redo, events_since, run ERC/DRC). UI: Project Home (recent projects via SQLite index), workspace layout with command palette, a read-only schematic view and PCB view rendering EAK-IR geometry (canvas/SVG, pan/zoom, selection with cross-probe by symbol_id), and an Engineering Monitor listing ERC/DRC findings. Frontend tests (vitest) plus Rust tests for the IPC layer; document how to run on all three OSes. Update STATUS.md, CHANGELOG.md and docs/REQUIREMENTS_TRACEABILITY.md truthfully in the same PR. Must build and test on Windows, Linux and macOS CI.
  Delivery: direct-PR: no-mistakes review broken on Windows (main firstmate decision 2026-11-13); 3-OS CI matrix is the mandatory merge gate plus firstmate diff review against method + frozen stack. yolo on, but merge also waits for the owner's dependency approval (requested 2026-11-13).
## Queued
- [ ] eak-desktop-schematic-edit - Interactive schematic editor and library browser (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Priority 1 of the 2026-11-14 release. Built on the unmerged shell branch fm/eak-desktop-shell (PR #12) while its dependency approval is pending. Native schematic editor: place/move/rotate/delete components; library browser + symbol integration; wire draw/edit with junction-correct connectivity (crossings never connect); power symbols + net labels; properties/values; multiple sheets where the engine supports them; zoom/pan/select/grid; undo/redo; save/reopen/revisions; live connectivity feedback + ERC panel with actionable findings. Must create, save, reopen and verify a real design. Delivery: direct-PR: no-mistakes review broken on Windows (until fixed); 3-OS CI matrix is the merge gate plus firstmate diff review against method + frozen stack; yolo on. Owner directive 2026-11-14 07:00 IST release (firstmate data/eak-final-directive-2026-11-14.md). UI built on eak-ux's design system (apps/desktop/src/design/, owned by eak-ux; consume, do not edit; file boundary agreed through main). Every mutation through eak-runtime typed commands on EAK-IR as the Human actor; a missing runtime command goes to main as needs-decision with the exact API, never edited in crates/. Generic product only (no ESP32/buck logic). No new dependency without owner approval. No fake functionality; honest maturity states in docs/REQUIREMENTS_TRACEABILITY.md. 3D deferred this release.
- [ ] eak-desktop-pcb-edit - Native 2-layer PCB editor blocked-by: eak-desktop-shell (repo: eak) (kind: ship) (priority: 1) (since 2026-11-13)
  Priority 2 of the 2026-11-14 release; may run in parallel with the schematic editor only after the shell lands. Board outline create/edit; footprint place/rotate/move with pads; layers; track routing + editing; vias; net highlight; ratsnest/unrouted visibility; copper zones/ground pour; design-rule config; clearance + width DRC with actionable findings; connectivity check; undo/redo; save/reopen/revisions; manufacturing export prep. Never fake routing or copper. 3D viewport dropped from this release. Delivery: direct-PR: no-mistakes review broken on Windows (until fixed); 3-OS CI matrix is the merge gate plus firstmate diff review against method + frozen stack; yolo on. Owner directive 2026-11-14 07:00 IST release (firstmate data/eak-final-directive-2026-11-14.md). UI built on eak-ux's design system (apps/desktop/src/design/, owned by eak-ux; consume, do not edit; file boundary agreed through main). Every mutation through eak-runtime typed commands on EAK-IR as the Human actor; a missing runtime command goes to main as needs-decision with the exact API, never edited in crates/. Generic product only (no ESP32/buck logic). No new dependency without owner approval. No fake functionality; honest maturity states in docs/REQUIREMENTS_TRACEABILITY.md. 3D deferred this release.
- [ ] eak-desktop-mfg-release - Manufacturing Center and installable desktop bundles blocked-by: eak-desktop-shell (repo: eak) (kind: ship) (priority: 2) (since 2026-11-13)
  Priority 3 of the 2026-11-14 release. Manufacturing Center UI (Gerbers/BOM/CPL/JLCPCB prep surfaced from eak-runtime outputs) and Tauri bundles for Windows, Linux and macOS built and startup-checked per platform in CI. CI/packaging is owned by eak-core: coordinate through main before touching CI. Exact install instructions and version info. Delivery: direct-PR: no-mistakes review broken on Windows (until fixed); 3-OS CI matrix is the merge gate plus firstmate diff review against method + frozen stack; yolo on. Owner directive 2026-11-14 07:00 IST release (firstmate data/eak-final-directive-2026-11-14.md). UI built on eak-ux's design system (apps/desktop/src/design/, owned by eak-ux; consume, do not edit; file boundary agreed through main). Every mutation through eak-runtime typed commands on EAK-IR as the Human actor; a missing runtime command goes to main as needs-decision with the exact API, never edited in crates/. Generic product only (no ESP32/buck logic). No new dependency without owner approval. No fake functionality; honest maturity states in docs/REQUIREMENTS_TRACEABILITY.md. 3D deferred this release.
- [ ] eak-desktop-agent-studio - Integrated AI assistant dock blocked-by: eak-desktop-shell (repo: eak) (kind: ship) (priority: 3) (since 2026-11-13)
  Priority 4 of the 2026-11-14 release (replaces Agent Studio). AI assistant dock consuming eak-intelligence's API; approval UI for agent CommandRequests so no model bypasses validation; AI edits only through typed commands. No provider/credential code in desktop beyond what eak-intelligence exposes. Delivery: direct-PR: no-mistakes review broken on Windows (until fixed); 3-OS CI matrix is the merge gate plus firstmate diff review against method + frozen stack; yolo on. Owner directive 2026-11-14 07:00 IST release (firstmate data/eak-final-directive-2026-11-14.md). UI built on eak-ux's design system (apps/desktop/src/design/, owned by eak-ux; consume, do not edit; file boundary agreed through main). Every mutation through eak-runtime typed commands on EAK-IR as the Human actor; a missing runtime command goes to main as needs-decision with the exact API, never edited in crates/. Generic product only (no ESP32/buck logic). No new dependency without owner approval. No fake functionality; honest maturity states in docs/REQUIREMENTS_TRACEABILITY.md. 3D deferred this release.
- [ ] eak-desktop-terminal-center - Terminal Center UI (Herdr-inspired, advanced experience) blocked-by: eak-desktop-shell (repo: eak) (kind: ship) (since 2026-11-13) (hold: parked: dropped from the 2026-11-14 release by owner directive) (hold-kind: parked)
  Owner: eak-desktop (UI only). Native Terminal Center for the Advanced/Developer experience, built over eak-core's Rust session/agent supervisor API (eak-core owns process supervision and session lifecycle; eak-ux owns interaction and visual design): multiple named, persistent, project-scoped sessions; create/switch/rename/terminate; live stdout/stderr; concurrent supervised agents showing project/task/model/session/ownership identity; cancel, timeouts and recovery state; command history; working-directory/project context; resource monitoring; one project's failure isolated from another. Herdr (github.com/herdrdev/herdr) is a concept reference only: verify its licence, never copy code, never a runtime dependency. Any PTY or terminal-rendering dependency (e.g. xterm.js) needs a written justification and owner approval first. Waits for: eak-core session/agent supervisor interface. Delivery: direct-PR while no-mistakes is broken on Windows; 3-OS CI is the merge gate plus firstmate diff review; yolo on. Frozen stack (Tauri 2, React+TS+Vite, no Python, no new dependency without owner approval). Generic product; secrets never shown in UI logs/diagnostics. Do not start until the main firstmate has delivered the agreed Rust interface contract this item consumes - the UI builds against that contract only and never invents a parallel supervisor, provider or credential model.
- [ ] eak-desktop-model-provider-center - Model Provider Center UI blocked-by: eak-desktop-shell (repo: eak) (kind: ship) (since 2026-11-13) (hold: parked: dropped from the 2026-11-14 release by owner directive; a minimal provider settings panel may return if time allows) (hold-kind: parked)
  Owner: eak-desktop (UI only). Model Provider Center over eak-intelligence's provider/model trait and credential store API: connect, authorise (API key or official OAuth where supported), custom/OpenAI-compatible/local endpoints, model discovery and capability display (context window, tool calling), connection test, default model, per-agent and per-task model choice, usage/token display, disconnect/revoke, and which model runs which agent. Secrets go only to OS credential storage via the intelligence API and never into UI state logs, project files or diagnostics; consent gate before any project data goes to an external model. Fully usable offline without cloud AI. Waits for: eak-intelligence provider/model + credential store interface. Delivery: direct-PR while no-mistakes is broken on Windows; 3-OS CI is the merge gate plus firstmate diff review; yolo on. Frozen stack (Tauri 2, React+TS+Vite, no Python, no new dependency without owner approval). Generic product; secrets never shown in UI logs/diagnostics. Do not start until the main firstmate has delivered the agreed Rust interface contract this item consumes - the UI builds against that contract only and never invents a parallel supervisor, provider or credential model.
## Done

---

# Home: eak-electronics

## In flight
- [ ] eak-jlcpcb-prep - JLCPCB-ready outputs, board parameters and DFM readiness report (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics. Owner required integration (corr 9da320bee2ceb4e1). Offline JLCPCB prep in eak-manufacturing: JLCPCB-format BOM+CPL, board dimension/layer/thickness/copper extraction, Gerber/drill inspection, DFM/readiness report against published JLCPCB capabilities with source+date, unsupported/unmapped parts list. Standard outputs stay fab-neutral. no-mistakes, yolo on.
- [ ] eak-lcsc-sourcing - LCSC sourcing records, compatibility checks and integration test matrix (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics. Owner required integration (corr 9da320bee2ceb4e1). Offline-first LCSC sourcing: parse documented LCSC product data into provenance/confidence records, symbol-footprint-datasheet compatibility and missing-data checks, BOM LCSC mapping, integration test matrix with offline fixtures. No new dependencies; live fetch pending main-firstmate decision. no-mistakes, yolo on.
- [ ] eak-stackup - Per-layer board stackup in EAK-IR with v1->v2 migration (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics, approved by main firstmate 2026-11-13 (corr a786b05f8a62be06): Board.stackup per-layer copper_oz/dielectric/er, default 2-layer, schema v1->v2 migration loading existing files unchanged, round-trip test. CopperLayer::Inner deferred. One small PR, merge when green. no-mistakes, yolo on.
- [ ] eak-power-calc - Buck converter power calculations and rating ERC (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics. From the owner's acceptance test (12-24 V -> 5 V 3 A synchronous buck board). calc.rs buck design calculations and ERC rating checks for regulators/inductors/FETs. Delivery no-mistakes, yolo on.
- [ ] eak-lib-expand - Expand built-in component library for real designs (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics. Self-directed under the owner's 2026-11-13 deadline directive (keep the domain moving toward an installable product). Expand crates/eak-core/src/builtin.rs content only (no type changes): common passives (R/C/L in 0402/0603/0805/1206), diodes (SOD-123, SMA), LEDs (0603/0805), BJTs/MOSFETs (SOT-23), common regulators/MCUs/connectors, with provenance and unverified ratings flagged. Delivery no-mistakes (product-facing), yolo on.
- [ ] eak-fab-detail - Thermal reliefs, silkscreen text and IPC rule verification (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-electronics. In eak-engineering/eak-validation/eak-manufacturing only: thermal-relief spokes for same-net pads inside zones (fill, connectivity and DRC aware), silkscreen reference-designator text via a built-in stroke font in Gerber silkscreen with clip against pads/soldermask openings, a DRC rule for silkscreen-over-pad, and an engineering note documenting each IPC/IEC table value with its verification status (do not mark verified without the standard text). Extend the end-to-end reference-board test so the package still reaches InternallyVerifiedReleaseCandidate. Delivery: product-facing EAK work, so no-mistakes pipeline to a PR; merge needs the owner's explicit approval (yolo off). Update STATUS.md, CHANGELOG.md and docs/REQUIREMENTS_TRACEABILITY.md truthfully in the same PR. Must build and test on Windows, Linux and macOS CI.

## Queued
- [ ] eak-lcsc-integration - LCSC sourcing + library integration: search, LCSC id/MPN mapping, datasheet links, spec inspection, footprint/package matching, availability/price via permitted API, BOM, alternatives, provenance blocked-by: eak-lcsc-sourcing (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Owner directive 2026-11-14 item 4. Reference github.com/NijoP/DESK component_intelligence/, library/, docs/ecosystem/LCSC_INTEGRATION_ARCHITECTURE.md (reference only; re-implement in Rust, no Python). Offline core; live HTTP optional via approved ureq+rustls only. Never invent specs. Separate from EDA engine and from JLCPCB prep.
- [ ] eak-actionable-findings - ERC/DRC findings actionable for editors: finding id, object ids/location, rule, measured vs required, suggested fix (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Owner directive 2026-11-14 part (a). Finding shape must be agreed with eak-core and eak-desktop via main before changing shared interfaces.
## Done

---

# Home: eak-intelligence

## In flight
- [ ] eak-intelligence-core - eak-intelligence crate: agent loop, providers, typed tools, approval flow (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-intelligence. Create crates/eak-intelligence (added to workspace members - that one-line Cargo.toml member edit is permitted): provider trait with OpenAI-compatible, Anthropic Messages and Ollama implementations (keys only from user configuration or environment, never committed), a cancellable agent loop with bounded turns, typed tool schemas generated from the eak-runtime Command enum plus read-only tools (netlist, ERC/DRC reports, calculators), every mutation submitted as an Agent actor CommandRequest that must carry an Approval covering its OperationClass, and agent modes (ask/plan/act). Tests use a scripted mock provider; no network in tests. Delivery: product-facing EAK work, so no-mistakes pipeline to a PR; merge green in-scope PR under delegated authority (yolo on). Update STATUS.md, CHANGELOG.md and docs/REQUIREMENTS_TRACEABILITY.md truthfully in the same PR. Must build and test on Windows, Linux and macOS CI.


  Intake: mode=direct-PR (direct-PR: no-mistakes review broken on Windows; main-firstmate decision 2026-11-13, 3-OS CI is the mandatory merge gate plus firstmate diff review against AGENTS.md method and frozen stack); was no-mistakes; yolo=on (owner delegated routine EAK merges 2026-11-13; merge green in-scope PR via fm-pr-merge).

## Queued
- [ ] research-browser - eak-intelligence optional research browser: bounded process supervisor, HTTP-first datasheet research, Lightpanda licence analysis blocked-by: eak-intelligence-core (repo: eak) (kind: ship) (since 2026-11-13)
  Owner: eak-intelligence. Routed by main firstmate 2026-11-13 (corr=6a0a76bb64c3fac5) as a required integration. Optional on-demand headless browser for engineering research: Rust process supervisor launching it through a bounded process interface (timeouts, resource limits, cleanup, failure recovery); HTTP/API first, browser only when interactive access is needed; datasheet/manufacturer/supplier research with structured extraction carrying source URL, timestamp and evidence record; all web content untrusted data, never instructions; core EAK fully offline without it. Evaluate Lightpanda as engine: AGPL, so external optional process only, never bundled or redistributed, no code copied; record the licence analysis. Offline fixtures for CI; network tests clearly marked. Frozen stack: no new dependency without owner approval. Intake: mode=direct-PR while no-mistakes review is broken on Windows (else no-mistakes); yolo=on (delegated). Generic only: no project-specific logic; wait for the domain knowledge-pack design before any classification or rules system; maturity vocabulary NOT_STARTED..BLOCKED. Dispatch after eak-intelligence-core lands. On delivery of the FetchedDocument path, notify eak-electronics (via the main firstmate parent channel) that it is available for LCSC/JLCPCB. HTTP stays behind the optional Cargo feature approved by the owner.
- [ ] dep-ureq-https - Decide: adopt ureq+rustls HTTPS client in eak-intelligence (repo: eak) (kind: docs) (since 2026-11-13)
  Resolution recorded by fm-owner-hold.
  Decision digest: 694ddc34dcb8c53e3c3ccd98fc18011d7294d13fa183555261be5f371dc9e70e
  Resolution mode: released

  Owner decision:
  Owner decision (relayed by main firstmate, corr=26216b4d91449cbe, 2026-11-13): ureq + rustls APPROVED on two conditions: (1) every new crate recorded with its licence in LICENSING.md from cargo metadata, no copyleft confirmed; (2) live HTTP optional behind a Cargo feature or runtime config, core builds/runs/tests fully offline, proven by a no-network test/CI step.

  Frozen-stack rule requires owner approval of any new dependency. eak-intelligence-core PR must not merge until this is decided. Writeup: data/eak-intelligence-core/dependency-decision.md. Recommendation: approve; fallback is local-only providers with remote providers reported incomplete.

- [ ] provider-interface-contract - Scout: minimal Rust provider/model trait + credential store API contract for eak-desktop/eak-ux blocked-by: eak-intelligence-core (repo: eak) (kind: scout) (since 2026-11-13)
  Routed by main firstmate 2026-11-13 (corr=fea8daa07064b5ce). Before building the universal provider system, send the main firstmate the minimal Rust interface eak-intelligence will expose: provider/model trait (registration, API key and official OAuth where supported, endpoint config, model discovery, capability detection incl. context window and tool calling, streaming, usage/token tracking, retry/rate-limit, per-task and per-agent model choice, connect/test/disconnect/revoke) and the credential store API (OS credential storage only; secrets never in logs, terminal output, project files, commits or agent-visible diagnostics; consent gate before project data goes to an external model). Build on the landed eak-intelligence provider trait. Include a written justification for any new dependency (e.g. an OS keyring crate) for owner approval. Deliverable: report for the main firstmate, no code. Then a ship implements it once the contract is agreed. Ownership: eak-core owns process/session supervision; eak-desktop the Provider Center UI; eak-ux experience. Stack frozen; memory cap 2 workers.
- [ ] universal-provider-system - Universal AI model provider and adapter system (eak-intelligence) blocked-by: provider-interface-contract (repo: eak) (kind: ship) (since 2026-11-13)
  Owner request 2026-11-13 (direct, unmarked): Universal AI Model Provider and Adapter System, a non-negotiable EAK product requirement. EAK must not be locked to any single AI company.
  Scope: independent provider adapters behind one shared model interface in crates/eak-intelligence; adding a provider must not touch the engineering core; existing agent loop, typed tools and approval flow use models only through it.
  Providers: Anthropic (Opus, Sonnet, others), OpenAI (GPT, Codex), xAI Grok, Moonshot Kimi, Google Gemini, DeepSeek, NVIDIA-hosted, other/future, self-hosted/local via compatible endpoints.
  Requirements: official API auth + OAuth where supported + secure API-key management; OpenAI-compatible and other standard protocols plus custom endpoints; model discovery and capability detection; streaming, tool calling, structured output where supported; context window, token usage, rate limit and cost tracking; health checks, error handling, controlled retries; per-agent and per-task model selection; secure credential storage, keys only from user config, never in logs, source, project files, commits or fixtures; all model actions via typed eak-runtime commands over EAK-IR, never bypassing verification or approval.
  Stack frozen: Rust backend; provider config/management UI in React+TS inside the Tauri 2 app. No Python, no microservices. Must pass Windows/Linux/macOS CI.
  Coordination via main firstmate (no cross-boundary edits): eak-core for EAK-IR/Command/crate API changes; eak-desktop for Tauri IPC bindings and screen placement; eak-ux for design-system compliance. Agree module ownership up front, no duplicate implementations.
  Testing/honesty: test at least two genuinely different providers end to end (auth, model selection, streaming, tool execution, failure handling, credential storage); never claim support for an untested adapter; STATUS.md and docs/REQUIREMENTS_TRACEABILITY.md truthful (unknown is never pass); no private credentials, no spending; live tests need owner-configured keys, otherwise report as a blocker.
  Plan: provider-interface-contract scout defines the trait + credential store contract and ownership split first (widened to this provider list and requirements); this ship implements the Rust side after the contract is agreed. Delivery: no-mistakes (product-facing), yolo on per registry. Host memory: one worker at a time, CARGO_BUILD_JOBS=2.
## Done

---

# Home: eak-ux

## Backlog

## In flight
## Queued
- [ ] ux-ds - EAK design system as code: apps/desktop/src/design/ tokens, dark+light themes, base primitives (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Owner final directive 2026-11-14 item 0. Due 22:30 IST 2026-11-13. Mode no-mistakes (product-facing), yolo on. Owns apps/desktop/src/design/ only.
- [ ] ux-ia - docs/design/INTERACTION_ARCHITECTURE.md: Cursor-inspired EDA layout, palette, shortcuts, Beginner/Advanced (repo: eak) (kind: ship) (priority: 0) (since 2026-11-13)
  Owner final directive 2026-11-14 item 0. Due 22:30 IST 2026-11-13. Mode no-mistakes (product-facing doc), yolo on.
- [ ] ux-accept - Acceptance review of desktop UI PRs against the design system; gaps to eak-desktop via main blocked-by: ux-ds (repo: eak) (kind: scout) (priority: 1) (since 2026-11-13)
  Directive item 0 follow-on: after the design system lands, review eak-desktop UI PRs against it.
## Done

---

# Home: eak-windows-optimizer

## Backlog

## In flight
## Queued
- [ ] winopt-phase2-approvals - Windows laptop optimization: approve Phase 2 changes (repo: firstmate) (kind: owner) (since 2026-11-13) (hold: fm-hold-v1:UGhhc2UgMSBkaWFnbm9zaXMgaXMgaW4gZGF0YS9vcHRpbWl6ZXIvcmVwb3J0Lm1kLiBDYXB0YWluJ3Mgd29yZCBuZWVkZWQgb246IFsxXSBlbmQgdGhlIGZvdXIgbGVmdG92ZXIgRWFzeUVEQSBQcm8gYmFja2dyb3VuZCBwcm9jZXNzZXMgLSBhYm91dCAxNyBHQiBjb21taXQgYW5kIDUgQ1BVIGNvcmVzLCBubyB3aW5kb3cgb3BlbjsgWzNdIHJlYm9vdCB0byBjbGVhciBhYm91dCAyLjQgR0Igb2YgbGVha2VkIGtlcm5lbCBtZW1vcnkgYW5kIGZpbmlzaCBwZW5kaW5nIHNlcnZpY2luZzsgWzddIHNldCBNeVNRTDgwLCBUdXJib1ZQTlNlcnZpY2UsIEFkb2JlQVJNc2VydmljZSB0byBtYW51YWwgc3RhcnQuIE1haW4gZmlyc3RtYXRlIGdvLWFoZWFkIG5lZWRlZCBvbiBzYWZlIGl0ZW1zIDQsIDUsIDYsIDgsIDkuIEl0ZW0gMiAtIHJlZHVjaW5nIHRoZSBmbGVldCdzIHByb2Nlc3MtbGF1bmNoIHJhdGUgLSBpcyB0aGUgbWFpbiBmaXJzdG1hdGUncyBvd24gY2FsbC4=) (hold-kind: owner)
  Owner hold set: 2026-11-13T12:42:18Z
## Done
