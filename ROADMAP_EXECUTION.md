# EAK execution roadmap

The live record of the EAK product transformation: what each goal needs, its current state, and the
evidence behind that state. It follows the captain's execution brief (§2 to §18). Goals start from
the brief's M0–M11 list and are re-sequenced only where the live code requires it. No goal is
dropped.

- **Repository:** https://github.com/NijoP/electronics-agent-kit-new-prototype
- **Roadmap keeper:** Firstmate crew worker `eak-roadmap`. Firstmate liaison: the main firstmate.
- **Last audited:** 2026-11-26T16:33Z, `main` at
  [500e938](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/500e938).
- **Rules for this file:** §1 sets the `*` rules and §3 sets the evidence rules. The change log
  (§9) is append-only.

`ROADMAP.md` (the older stage table) and `STATUS.md` (capability labels) still exist. When this file
and either of them disagree, use this file for goal state and `STATUS.md` for per-capability
verification labels. Then fix the stale one.

---

## 0. Now / Next / Blocked / Recently completed

**Completed goals (`*`): none.** No goal meets §1.2 today. Merged router and verification work is
**CI VERIFIED**, not COMPLETE: its milestone acceptance (the constrained board, PR #142) is still red.

| | Items |
|---|---|
| **Now** | M0.1, M0.2, M0.5 and M0.6: this roadmap, IN REVIEW. M7.4/M7.5: [PR #142](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/142), the constrained acceptance board. It is red by design, and the electronics lead is fixing the router rather than the test. M8.3: [PR #143](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/143), DC voltage drop. It is red because the rule-ID registry test `rule_config.rs::known_rule_ids_are_the_rules_the_checks_declare` does not know DRC-028. M3.3/M3.4: WEFT activity view, launch and review ([#126](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/126), [#129](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/129), [#130](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/130)), being landed by `eak-v2-proposal`. M1.3: DESK study (scout `eak-desk-study`). M4.1: requirements model (worker `eak-req-model`). |
| **Next** | M0.3 task contracts and M0.4 mutation authority (firstmate). M1.1 Traycer/REA and M1.2 Firstmate research. M1.4 surface map. M2.1 Agents workspace entry point (needs M1.5 UX spec). M7.11 diverse acceptance suite (after M7.4). |
| **Blocked** | M9.6 fabricate a prototype and M9.7 assemble/test it: **BLOCKED, awaiting captain authorization** (standing restriction: no PCB fabrication, ordering or hardware testing until the captain allows it). M9.8 waits on M9.6/M9.7. Old hardware-board PRs [#9](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/9), [#10](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10) and [#53](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/53) are held for the same reason. |
| **Recently completed** | Nothing reached `*`. Merged and CI-green on `main` today: #133, #134, #135, #136, #137, #138, #139, #140, #141 (§7). Released: [v0.2.0-rc.1](https://github.com/NijoP/electronics-agent-kit-new-prototype/releases/tag/v0.2.0-rc.1) at 6b71bb2 (unsigned pre-release). |

---

## 1. The `*` completion protocol (brief §2)

### 1.1 Marker form
Every goal uses one form:

```md
- [ ] Mn.m — Title · `STATE` · owner · deps · evidence …
- * Mn.m — Title · `COMPLETE (*)` · owner · evidence record linked
```

`- [ ]` means the goal is not complete, whatever its state. `- *` means **every** acceptance
criterion has passed **and** the evidence record (§3.2) is linked. A goal is never starred because work
started, compiled, merged or went green on one PR. A milestone gate is starred only when every goal in it
is starred and the gate's own condition is shown.

### 1.2 Evidence needed before a `*`
1. Implementation paths and the commit or PR that landed them.
2. Test commands and a **real** CI run on `main` with its result. Execution evidence is never invented.
3. UI capability: a real Tauri/WebView window (the ui-evidence workflow at minimum), not jsdom alone.
4. Engineering capability: known inputs, calculation and check provenance, tests, and an independent
   cross-check where relevant.
5. Negative tests: missing inputs, invalid state, a blocked operation and the error path.
6. Limitations, or `none known within the tested scope`.
7. Hardware-facing release goals: a real hardware or manufacturing validation record.

### 1.3 States
`NOT STARTED` → `IN PROGRESS` → `IN REVIEW` (PR open) → `CI VERIFIED` (on `main`, CI green) →
`DESKTOP VERIFIED` (real-window evidence) → `ENGINEERING VERIFIED` → `MANUFACTURING VERIFIED` →
`COMPLETE (*)`. `BLOCKED` names what it waits on. A goal only claims the levels its acceptance
needs: a docs goal can go from CI VERIFIED to COMPLETE once reviewed. These states are not
collapsed into one green colour.

### 1.4 Reopening
If a regression breaks a starred goal, the `*` is removed. A change-log line is added with the
regression link, and the goal returns to `IN PROGRESS`. The goal's old evidence stays in its record,
struck through, never deleted.

---

## 2. Owners

| Owner | Scope |
|---|---|
| main firstmate (liaison) | dispatch, merge sequencing, M0.3, M0.4, captain escalations |
| `eak-roadmap` (this worker) | this file: M0.1, M0.2, M0.5, M0.6, roadmap upkeep |
| electronics lead, secondmate `eak-electronics` | routing, verification, manufacturing: M7, M8, M9.1–M9.5 |
| library lead `eak-library` | library and sourcing: M5 |
| `eak-v2-proposal` | WEFT landing (#126/#129/#130), proposal work in M3 |
| scout `eak-desk-study` | M1.3 DESK study |
| `eak-req-model` | M4.1 requirements model |
| captain (founder) | M9.6/M9.7 authorization, M10.10 release decision, M11.5 sign-off |
| `unassigned` | firstmate dispatches it. Not started until an owner is named here |

---

## 3. CI evidence convention and stable link format (M0.5)

### 3.1 What counts as CI evidence
- **The `ci` workflow on a `main` commit that contains the change.** Its jobs are `linux`, `render-qa`,
  `cross-platform (windows-latest)`, `cross-platform (macos-latest)` and
  `cross-platform (macos-15-intel)`. Together they cover fmt, clippy `-D warnings`,
  `cargo test --workspace --locked`, vitest, the Python-free check, the engine boundary and the docs
  checks. Every job must be `success`. A PR run supports `IN REVIEW` only. `CI VERIFIED` needs the
  run on `main`.
- **The `ui-evidence` workflow** (real window under Xvfb) for UI goals. It runs only when
  `apps/desktop/**` changes, so cite the run for the commit that changed the UI, plus the scenario
  file. A UI goal is not `DESKTOP VERIFIED` without its own scenario rows.
- **Named tests.** Cite them as `path::test_name`, for example
  `crates/eak-validation/tests/acceptance_route.rs`. Mark tests that use fakes, fixtures or fake
  binaries as `(fixture)`. A fixture is never cited as live integration.
- **Only cite a run after reading its result** with `gh run view <id>` (or `--log-failed`). Record
  the conclusion as it was printed.
- No builds or tests run on the founder's laptop. All execution evidence comes from GitHub Actions.

### 3.2 Evidence record for each goal
When a goal leaves `NOT STARTED`, it gets indented fields under its line. A missing field means
"none yet". A field is never left blank to suggest a pass.

```md
  - paths: crates/…, apps/desktop/src/…
  - landed: [#NNN](…/pull/NNN) · [abc1234](…/commit/abc1234)
  - ci: [ci 45858355299](…/actions/runs/45858355299) success (main abc1234, 2026-11-26)
  - ui: [ui-evidence 45823077028](…/actions/runs/45823077028) success · scenarios/<scenario>.json
  - engineering: inputs, provenance, cross-check
  - negative: tests for missing input, invalid state, blocked op, error path
  - limits: … | none known within the tested scope
  - next: …
```

### 3.3 Stable link format
Every link is a full `https://github.com/NijoP/electronics-agent-kit-new-prototype/…` URL:

| Kind | Form |
|---|---|
| PR | `[#142](…/pull/142)` |
| Commit | `[500e938](…/commit/500e938)`: the 7-character SHA as printed by git |
| CI run | `[ci <run-id>](…/actions/runs/<run-id>) <conclusion>` |
| CI job | `[<job name>](…/actions/runs/<run-id>/job/<job-id>)` |
| UI evidence | `[ui-evidence <run-id>](…/actions/runs/<run-id>) <conclusion>`, then the scenario file |
| Release | `[v0.2.0-rc.1](…/releases/tag/v0.2.0-rc.1)` |
| Repo file | a path relative to the repo root, in backticks |

Run IDs and SHAs do not change, so these links stay stable. Never link to a branch name as
evidence, because a branch moves.

---

## 4. Baseline report (M0.1, brief §0.2), 2026-11-26T16:33Z

**Checkout.** `main` = [500e938](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/500e938)
(merge of #140). The working tree is clean. 56 remote branches, 53 of them `fm/*`. Schema
`SCHEMA_VERSION = 8`. The highest ADR is 0056 (`docs/decisions/`).

**CI on `main`.** [ci 45858355299](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858355299)
at 500e938 succeeded on all 5 jobs. The last `ui-evidence` run on `main` was
[ui-evidence 45823077028](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45823077028)
at c107f52, success. Later `main` commits did not touch `apps/desktop/**`. The ui-evidence
workflow runs 34 scenarios.

**Open PRs.**

| PR | Branch | Owner | State at audit |
|---|---|---|---|
| [#143](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/143) DRC-028 DC voltage drop | `fm/eak-verify-vdrop` | electronics lead | CI **red**: `crates/eak-validation/tests/rule_config.rs:206` `known_rule_ids_are_the_rules_the_checks_declare` fails on all platforms ([run 45858036095](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858036095)) |
| [#142](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/142) constrained acceptance board | `fm/eak-route-dp` | electronics lead | red by design (§7). New head 890bcc9; its CI was running at audit time. **Must not be weakened.** |
| [#130](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/130) WEFT conflicts and review | `fm/eak-v2-weft-review` | `eak-v2-proposal` | CI running at audit time |
| [#129](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/129) WEFT launch integration | `fm/eak-v2-weft-launch` | `eak-v2-proposal` | CI running at audit time |
| [#126](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/126) agent activity view | `fm/eak-v2-activity` | `eak-v2-proposal` | CI running at audit time |
| [#53](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/53), [#10](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10), [#9](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/9) hardware boards | old | captain | held: need hardware authorization (`docs/qa/old-pr-triage.md`) |
| [#6](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/6) buck calcs and rating ERC | `fm/eak-power-calc` | unassigned | held: rule IDs ERC-014..016 were reused on `main`, so it needs new IDs (triage) |
| [#1](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/1) Ponytail method in AGENTS.md | `fm/eak-dev-method` | captain | held: changing agent rules is the captain's choice |

**Active workers** (as briefed to this worker by firstmate; this worker does not inspect other
lanes): electronics lead `eak-electronics`, library lead `eak-library`, `eak-v2-proposal`,
`eak-desk-study`, `eak-req-model` and `eak-roadmap`.

**Application as it stands** (labels from `STATUS.md`):
- Tauri desktop: Project Home, journey bar, Problems panel, schematic, PCB, 2D/3D, library,
  checks, outputs and the Terminal workspace. 34 real-window scenarios.
- One EAK-IR (v8) through `Engine::execute`.
- Propose mode and the in-app proposal review dialog (ADR-0051).
- WEFT agent-to-agent tools (ADR-0053). AI tool adapters with Settings › AI tools, and an AI
  session view with a live log. Both use probes and fake binaries only.
- Multilayer autorouter: coupled diff pairs, push-and-shove, blind/buried vias and per-net
  inflation retry.
- Verification engines DRC-024..027 and ERC-025.
- Release integrity: revision-bound packages and Gerber/drill parse-back. Outputs remain **DRAFT**.
- Library: MPN part model, KiCad import/export, local DB, user library, datasheet refs, and a
  not-connected LCSC slot.

**Gaps between the brief and the code:**
- No Agents workspace separate from settings (M2.1).
- No real-CLI adapter tests; fakes only (M2.3/M2.4/M2.9).
- No keyring-backed login profiles for coding agents (M2.5).
- No requirements intake UI. The requirements model in `eak-intelligence::systems` is
  code-reviewed only (M4.1/M4.2).
- No domain classifier wired to the UI (M4.3).
- No real LCSC source (M5.2) and no JLCPCB mapping (M5.3).
- No block-spec schematic compilation (M6).
- The router fails a dense constrained board (M7.4).
- Voltage drop has not landed (M8.3).
- No simulation scope document (M9.1).
- No fabricated board (M9.6).
- No code signing (M10.6).
- None of the §16 research or architecture documents exist yet (Appendix B).

**Immediate preserve and continue:** let #142 and #143 continue under the electronics lead, and
let WEFT land under `eak-v2-proposal`. Do not edit their files. **First slices being started:**
this roadmap (M0), M1.3 DESK study and M4.1 requirements model.

---

## 5. Milestones and goals

The `Deps` lists use goal IDs. "groundwork" lists merged work that a goal builds on, which is not
completion.

### M0 — Baseline, safety and project execution control
- [ ] M0.1 — Audit repository, branches, open PRs, CI, working tree, Firstmate workers and current product baseline · `IN REVIEW` · eak-roadmap · deps: none
  - landed: this roadmap's PR (branch `fm/eak-roadmap`); baseline in §4
  - limits: the worker list is as briefed by firstmate and was not inspected independently
  - next: merge; re-audit at every roadmap event
- [ ] M0.2 — Create `ROADMAP_EXECUTION.md` with the entire roadmap, dependencies, owners, evidence fields and `*` rules · `IN REVIEW` · eak-roadmap · deps: M0.1
  - paths: `ROADMAP_EXECUTION.md`, README link
  - next: firstmate review, merge, then CI on `main`
- [ ] M0.3 — Define task contracts, ownership boundaries, scout/ship modes and safe branch/worktree discipline for the current fleet · `NOT STARTED` · main firstmate · deps: M0.1
  - groundwork: firstmate launch briefs (scout vs ship, one worktree per task, inbox steering)
- [ ] M0.4 — Define EAK design mutation authority, project revision binding, single-writer policy, proposal lifecycle and approval semantics · `NOT STARTED` · main firstmate · deps: M0.1
  - groundwork: `AGENTS.md` non-negotiables, ADR-0039 engine boundary, ADR-0051 propose mode (stale apply refused), ADR-0020 TTY approval
- [ ] M0.5 — CI evidence convention and stable link format · `IN REVIEW` · eak-roadmap · deps: M0.2
  - paths: §3 of this file
- [ ] M0.6 — Preserve and map router PRs #138–#142 (and #143) with owners and status · `IN REVIEW` · eak-roadmap · deps: M0.1
  - paths: §7 of this file

**Gate M0:** the current system is understood, active work is safe, the canonical roadmap is
running, and no destructive task has been performed. Gate state: **open** (M0.3 and M0.4 not started).

### M1 — Product and architecture discovery
- [ ] M1.1 — Traycer research using REA; evidence-based report `docs/research/TRAYCER_REVERSE_ENGINEERING.md` · `NOT STARTED` · unassigned · deps: M0.1
  - groundwork: firstmate-side Traycer review (§3, §8, §10, §11 of the eak-traycer-review report); not yet in the repo, not REA-based
- [ ] M1.2 — Deep Firstmate repository study; `docs/research/FIRSTMATE_REVERSE_ENGINEERING.md` · `NOT STARTED` · unassigned · deps: M0.1
- [ ] M1.3 — DESK workflow/domain study; `docs/research/DESK_TO_EAK_DOMAIN_WORKFLOW.md` · `IN PROGRESS` · eak-desk-study · deps: M0.1
- [ ] M1.4 — Map current EAK surfaces, user flows, feature status and technical debt from source · `NOT STARTED` · unassigned · deps: M0.1
  - groundwork: `STATUS.md`, `FEATURE_MATRIX.md` (older rows need a re-audit), `docs/qa/UI_INVENTORY.md`, the Phase-1 DoD audit
- [ ] M1.5 — Consolidated product architecture, design principles and staged UX spec · `NOT STARTED` · unassigned · deps: M1.1, M1.2, M1.3, M1.4
  - groundwork: accepted vision `docs/product/eak-product-vision.html`, design system `docs/product/README.md`
- [ ] M1.6 — Traceability from product requirements to roadmap features, tests and evidence · `NOT STARTED` · unassigned · deps: M0.2, M1.5
  - groundwork: `docs/REQUIREMENTS_TRACEABILITY.md` (source-of-truth spec rows)

**Gate M1:** the reports separate direct observation from inference, cite concrete source paths,
name the actual EAK gaps and give a feasible order. Gate state: **open**.

### M2 — Agent workspace foundation and real runtime integration
- [ ] M2.1 — Real Agents workspace entry point separate from model/provider settings · `NOT STARTED` · unassigned · deps: M1.5
  - groundwork: Settings › AI tools [#125](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/125), AI session view [#131](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/131), activity view [#126](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/126) (open)
- [ ] M2.2 — Agent roster, durable sessions, task list/graph, activity timeline, session detail · `NOT STARTED` · unassigned · deps: M2.1, M3.5
- [ ] M2.3 — Real Claude Code adapter, tested against an installed official CLI/account flow · `IN PROGRESS` · unassigned · deps: M2.9
  - groundwork: `crates/eak-agents` probe and Connect (#125); CI-verified with fake binaries only `(fixture)`
  - limits: never run against a real installed CLI
- [ ] M2.4 — Real Codex adapter, tested against an installed official CLI/account flow · `IN PROGRESS` · unassigned · deps: M2.9
  - groundwork and limits: as M2.3
- [ ] M2.5 — Account login vs API-key connection profiles with secure keyring storage · `NOT STARTED` · unassigned · deps: M2.3
  - groundwork: keyring-backed distributor credentials (ADR-0032), model provider settings
- [ ] M2.6 — Provider/runtime/model discovery and a real connection test with billing/auth identity · `NOT STARTED` · unassigned · deps: M2.5
  - groundwork: staged provider connection test (code-reviewed only)
- [ ] M2.7 — Terminal/session binding and native PTY interaction from the Agents workspace · `NOT STARTED` · unassigned · deps: M2.1
  - constraint: the Terminal workspace stays functional and visually unchanged; `terminal.test.tsx` is unchanged
- [ ] M2.8 — Lifecycle start/stop/interrupt/resume/restart as supported, with real status and errors · `IN PROGRESS` · unassigned · deps: M2.3, M2.4
  - groundwork: Restart in the AI session view (#131)
- [ ] M2.9 — Real integration tests and capability contracts (fake-binary tests stay unit coverage) · `NOT STARTED` · unassigned · deps: M1.1

**Gate M2:** the founder can connect supported coding agents, start real sessions, see real output,
inspect state, stop or interrupt them, and recover sessions, all without mock data. Gate state: **open**.

### M3 — Multi-agent task orchestration and review
- [ ] M3.1 — Structured task contract and scout/ship modes · `NOT STARTED` · unassigned · deps: M0.3
- [ ] M3.2 — Parent/child delegation, bounded workers and a single liaison · `NOT STARTED` · unassigned · deps: M3.1
- [ ] M3.3 — Agent messaging with queued/delivered/reply/expiry semantics · `IN PROGRESS` · eak-v2-proposal · deps: M3.1
  - groundwork: WEFT [#117](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/117) (`eak-harness/src/weft.rs`, `tests/weft.rs` with two real `eak-mcp` clients); #129 open
- [ ] M3.4 — Event-driven supervision and true runtime status classification · `IN PROGRESS` · eak-v2-proposal · deps: M3.3
  - groundwork: #126 activity view from project journals (open)
- [ ] M3.5 — Persistent task/session/event ledger and restart reconciliation · `IN PROGRESS` · unassigned · deps: M3.4, M0.4
  - groundwork: `eak-harness` `agent_task`, `task_journal` (ADR-0036); code-reviewed in parts
- [ ] M3.6 — Project-scoped context, permissions and secrets boundary · `NOT STARTED` · unassigned · deps: M0.4, M2.5
- [ ] M3.7 — Artifact/result panel, transcript, tool-call trace and failure diagnostics · `NOT STARTED` · unassigned · deps: M2.2
- [ ] M3.8 — Engineer approval/decision requests and escalation flows · `IN PROGRESS` · eak-v2-proposal · deps: M0.4
  - groundwork: proposal review dialog [#111](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/111) (ui-evidence `scenarios/g4-proposal.json`); #130 open; desktop approval (Traycer review slice S11) not started
- [ ] M3.9 — Isolated code-worker branches/worktrees and conflict-aware integration · `NOT STARTED` · unassigned · deps: M3.2
- [ ] M3.10 — Design proposal isolation, object-level diff and stale-revision conflict handling · `IN PROGRESS` · eak-v2-proposal · deps: M0.4
  - groundwork: propose mode [#106](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/106) refuses an apply at a stale revision (ADR-0051)

**Gate M3:** a real multi-agent engineering task can be delegated, supervised, recovered and
reviewed. No worker can silently mutate the authoritative design or exceed its authority. Gate state: **open**.

### M4 — Engineering requirements, knowledge and domain intelligence
- [ ] M4.1 — Structured requirements model with units, source, status and completeness · `IN PROGRESS` · eak-req-model · deps: M1.3
  - groundwork: `eak-intelligence::systems` typed requirements (ADR-0028, code-reviewed only)
  - constraint: an IR change needs an ADR, a migration and a round-trip test
- [ ] M4.2 — Requirements intake and project readiness/blocking UI · `NOT STARTED` · unassigned · deps: M4.1
- [ ] M4.3 — Deterministic domain/archetype classifier and ambiguity review · `NOT STARTED` · unassigned · deps: M4.1, M1.3
  - groundwork: domain registry (ADR-0028), `DomainPanel.tsx`
- [ ] M4.4 — Composable domain overlays and a versioned engineering rule registry · `NOT STARTED` · unassigned · deps: M4.3
  - groundwork: rule configuration and the rule-ID registry (ADR-0046)
- [ ] M4.5 — Port validated DESK taxonomy/knowledge with provenance and licensing review · `NOT STARTED` · unassigned · deps: M1.3
- [ ] M4.6 — Design profile compiled from confirmed/derived values only · `NOT STARTED` · unassigned · deps: M4.1, M4.4
- [ ] M4.7 — Traceability requirements → constraints → parts → blocks → nets → checks · `NOT STARTED` · unassigned · deps: M4.6, M6.1
- [ ] M4.8 — Rules/knowledge editor or inspectable knowledge browser, with validation tooling · `NOT STARTED` · unassigned · deps: M4.4

**Gate M4:** EAK classifies a real board request, exposes missing values, compiles a traceable
profile and blocks unsafe or under-specified operations. Gate state: **open**.

### M5 — Component intelligence, datasheets and BOM
- [ ] M5.1 — Component-family requirements and role-driven candidate search · `IN PROGRESS` · eak-library · deps: M4.1
  - groundwork: MPN discovery [#114](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/114), library DB search [#122](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/122)
- [ ] M5.2 — Reliable LCSC source integration and matching with evidence/timestamps · `NOT STARTED` · eak-library · deps: M5.1
  - groundwork: LCSC not-connected slot [#120](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/120); constraint: no LCSC private APIs
- [ ] M5.3 — JLCPCB assembly-part mapping and provider status reporting · `NOT STARTED` · eak-library · deps: M5.2
  - groundwork: assembly classification from user attestation (ADR-0034)
- [ ] M5.4 — Datasheet ingestion/parsing with source/page traceability and confidence · `IN PROGRESS` · eak-library · deps: M5.1
  - groundwork: datasheet references and typed specs [#116](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/116); no parsing yet
- [ ] M5.5 — Verified pin maps, symbol/footprint/pad mapping and package validation · `IN PROGRESS` · eak-library · deps: M5.4
  - groundwork: footprint verification FP-001..010 [#113](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/113); personal-library editing [#128](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/128)
- [ ] M5.6 — Candidate comparison on electrical, mechanical, thermal, supply and cost criteria · `NOT STARTED` · eak-library · deps: M5.1, M5.4
- [ ] M5.7 — Safe alternative/equivalence checks with explicit unknown states · `NOT STARTED` · eak-library · deps: M5.6
- [ ] M5.8 — Passive derivation/calculation records and structured BOM approvals · `NOT STARTED` · unassigned · deps: M4.6
  - groundwork: held [#6](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/6) (buck derivations, not ported)
- [ ] M5.9 — Supplier stock/price freshness semantics, inventory states and sourcing report · `NOT STARTED` · eak-library · deps: M5.2
  - groundwork: `SupplierOffer` and stale labels (ADR-0028/0032, code-reviewed only)

**Gate M5:** a real BOM is assembled from sourced candidate records, with datasheet, pin and
footprint evidence, derived passives, procurement states and no fictional availability. Gate state: **open**.

### M6 — Requirements-driven schematic and design compilation
- [ ] M6.1 — Functional block planning tied to requirements and chosen parts · `NOT STARTED` · unassigned · deps: M4.6, M5.6
- [ ] M6.2 — Machine-readable block specs and typed schematic compilation · `NOT STARTED` · unassigned · deps: M6.1
- [ ] M6.3 — Pin/net-level verification against the intended block/netlist · `NOT STARTED` · unassigned · deps: M6.2
- [ ] M6.4 — Schematic change proposals with readable object diffs and impact summaries · `IN PROGRESS` · eak-v2-proposal · deps: M3.10
  - groundwork: proposal review groups rows by object (#111)
- [ ] M6.5 — Extend ERC coverage and gate traceability · `IN PROGRESS` · electronics lead · deps: M4.4
  - groundwork: ERC with `not_implemented`/`limitations` reporting; ERC-025 (#141)
- [ ] M6.6 — Validate the complete workflow on representative embedded, power, RF and mixed-signal boards · `NOT STARTED` · unassigned · deps: M6.3, journeys A–C

**Gate M6:** a confirmed requirements/BOM set produces or guides a real schematic, with every
part, pin and net traceable and all required ERC gates truly evaluated. Gate state: **open**.

### M7 — Multilayer PCB engine and advanced routing
- [ ] M7.1 — Complete PR #138/#139 differential-pair work after verifying CI and integration · `CI VERIFIED` · electronics lead · deps: none
  - landed: [#138](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/138) · [ef566c8](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/ef566c8); [#139](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/139) · [a202556](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/a202556)
  - ci: [ci 45844579169](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45844579169) success (main ef566c8); [ci 45853087718](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45853087718) success (main a202556)
  - limits: integration on a dense constrained board is not yet proven (M7.4)
  - next: starred only when #142 passes with the pair coupled and within skew
- [ ] M7.2 — Complete and verify PR #140 blind/buried via routing and stack-up-valid via spans · `CI VERIFIED` · electronics lead · deps: none
  - landed: [#140](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/140) · [500e938](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/500e938)
  - ci: [ci 45858355299](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858355299) success (main 500e938, all 5 jobs)
  - next: acceptance on the M7.11 suite (blind/buried board)
- [ ] M7.3 — Complete and verify PR #141 via current, junction temperature and insulation/spacing checks · `CI VERIFIED` · electronics lead · deps: none
  - landed: [#141](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/141) · [0df008b](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/0df008b) (DRC-027, ERC-025, DRC-002 per declared insulation)
  - ci: [ci 45855454190](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45855454190) success (main 0df008b)
  - limits: IPC/IEC tables not checked against the standard text; engineering cross-check not recorded
- [ ] M7.4 — Complete the PR #142 constrained acceptance-board failure without weakening it · `IN PROGRESS` · electronics lead · deps: M7.5
  - landed: open [#142](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/142) (`crates/eak-validation/tests/acceptance_route.rs`: a 4-layer USB device with ESP32-C3-MINI-1 at 0.8 mm pitch and 26 GND pads, a 90 Ω pair with 0.05 mm max skew, +5V 1 A / +3V3 0.5 A, and GND/+3V3 planes)
  - limits: red by design. The router applied the widest net's clearance everywhere, and the spacing pre-check ignored declared insulation/creepage
  - next: router fix lands, then the test passes **unchanged**
- [ ] M7.5 — Per-net width and clearance semantics, including closely spaced module pads · `IN PROGRESS` · electronics lead · deps: none
  - landed: [3c9901f](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/3c9901f) per-net inflation retry for fine-pitch escapes and insulation-aware routing spacing (in #140)
  - next: proven by M7.4
- [ ] M7.6 — Real multilayer autorouting with layer transitions/via selection and robust rip-up/reroute · `IN PROGRESS` · electronics lead · deps: M7.2
  - landed: [#135](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/135) · [58f4fe3](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/58f4fe3), determinism [#136](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/136) · [cea00f7](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/cea00f7)
  - ci: [ci 45832122842](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45832122842) success; [ci 45847891125](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45847891125) success
  - limits: robustness on dense boards not shown (M7.4, M7.11)
- [ ] M7.7 — Pair-aware differential routing, impedance and skew/length constraints across layers · `IN PROGRESS` · electronics lead · deps: M7.1
  - landed: [#133](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/133) · [73be8a3](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/73be8a3) (ADR-0055/0056); ci: [ci 45841440634](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45841440634) success
- [ ] M7.8 — RF keep-out/antenna/return-path checks and reference-plane awareness · `IN PROGRESS` · electronics lead · deps: M7.6
  - landed: RF keep-out test in [#137](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/137) · [969e3c5](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/969e3c5); ci: [ci 45849973705](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45849973705) success
  - limits: no return-path or reference-plane continuity check yet
- [ ] M7.9 — High-speed rules where inputs/models are defined · `NOT STARTED` · electronics lead · deps: M7.7
- [ ] M7.10 — Copper plane, connectivity, stitching, thermal and power-distribution checks · `IN PROGRESS` · electronics lead · deps: M7.6
  - groundwork: planes and stack-up (E5, ui-evidence `scenarios/e5-planes.json`); zone stitching and dead-copper removal (ADR-0030, code-reviewed only)
- [ ] M7.11 — Diverse constrained-board acceptance suite (dense MCU pads, blind/buried, diff pair, RF, high-current, high-voltage, mixed-signal, multilayer) · `NOT STARTED` · electronics lead · deps: M7.4
- [ ] M7.12 — Deterministic route quality and performance on measured boards; benchmark report · `NOT STARTED` · electronics lead · deps: M7.11
  - groundwork: determinism evidence in CI output (#136)

**Gate M7:** all required nets route on supported stacks, constraints and keep-outs are respected,
geometry and readback validate, the acceptance boards pass, and no rule was weakened. Gate state: **open** (#142 red).

### M8 — Comprehensive engineering verification
- [ ] M8.1 — Complete electrical rule coverage with explicit UNKNOWN/NOT_IMPLEMENTED reporting · `IN PROGRESS` · electronics lead · deps: M4.4
  - groundwork: reports list `not_implemented` and `limitations`; engineering gates PASS/FAIL/UNKNOWN/NOT_RUN (ADR-0021)
- [ ] M8.2 — Clearance/creepage/insulation rules with standard provenance and verified tables · `IN PROGRESS` · electronics lead · deps: M8.1
  - landed: DRC-002 per declared insulation (#141)
  - limits: tables not verified against the standard text
- [ ] M8.3 — Current capacity, via capacity, voltage drop and power-distribution verification · `IN PROGRESS` · electronics lead · deps: M7.10
  - landed: DRC-027 via current (#141); open [#143](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/143) DRC-028 voltage drop, CI red ([run 45858036095](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858036095): rule-ID registry test)
- [ ] M8.4 — Thermal/junction-temperature and copper-heating analysis with defensible assumptions · `IN PROGRESS` · electronics lead · deps: M8.3
  - landed: ERC-025 junction temperature (#141)
- [ ] M8.5 — Differential pair/impedance/SI checks for declared supported stack-ups · `IN PROGRESS` · electronics lead · deps: M7.7
  - landed: DRC-024..026 (#137)
- [ ] M8.6 — Mechanical/3D/clearance/cutout/height/connector/enclosure checks · `IN PROGRESS` · electronics lead · deps: M4.1
  - groundwork: MECH checks; one board geometry for DRC/2D/3D [#107](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/107)
- [ ] M8.7 — DFM/fabricator rule profiles and fabrication-output integrity checks · `IN PROGRESS` · electronics lead · deps: M9.3
  - groundwork: release integrity [#123](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/123) (ADR-0054)
- [ ] M8.8 — Unified Checks workspace with object-linked findings and remediation workflow · `IN PROGRESS` · unassigned · deps: M8.1
  - groundwork: Findings panel (ui-evidence `scenarios/b5-findings.json`), Problems panel [#105](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/105) (`scenarios/g2-problems.json`)
- [ ] M8.9 — Independent verification of agent-generated design proposals · `IN PROGRESS` · eak-v2-proposal · deps: M3.10
  - groundwork: proposals carry a check delta (ADR-0051)

**Gate M8:** every applicable required check has a real evaluator and traceable evidence, missing
inputs stay visibly UNKNOWN or BLOCKED, and negative tests prove fail-closed behaviour. Gate state: **open**.

### M9 — Simulation, manufacturing and real-board validation
- [ ] M9.1 — Simulation workflow integration and documented supported model scope · `NOT STARTED` · electronics lead · deps: M1.4
  - groundwork: `crates/eak-simulation`, `SimulationPanel.tsx` (honesty pass F9)
- [ ] M9.2 — Revision-bound BOM, Gerber, drill, CPL, netlist and documentation package · `IN PROGRESS` · electronics lead · deps: M8.7
  - groundwork: revision-bound packaging (#123); output documents D3 (ui-evidence `scenarios/d3-output.json`); outputs remain DRAFT
- [ ] M9.3 — Output parsing/readback and cross-file consistency checks · `IN PROGRESS` · electronics lead · deps: M9.2
  - groundwork: Gerber/drill parse-back (#123)
- [ ] M9.4 — Manufacturing approval/release flow with an immutable evidence package · `IN PROGRESS` · electronics lead · deps: M9.3, M3.8
  - groundwork: release check surface (ui-evidence `scenarios/e7-release.json`)
- [ ] M9.5 — Representative reference designs routed through EAK end to end · `NOT STARTED` · electronics lead · deps: M6.6, M7.11
- [ ] M9.6 — Fabricate at least one EAK-generated prototype through a real manufacturer · `BLOCKED` (awaiting captain authorization) · captain · deps: M9.5
- [ ] M9.7 — Assemble, inspect and run documented electrical tests on the fabricated board · `BLOCKED` (awaiting captain authorization) · captain · deps: M9.6
- [ ] M9.8 — Record rework/failure lessons and compare predictions with measurements · `BLOCKED` (waits on M9.6/M9.7) · unassigned · deps: M9.7
- [ ] M9.9 — Prototype/manufacturing validation status separate from software test status · `NOT STARTED` · unassigned · deps: M0.5
  - groundwork: `STATUS.md` verification labels already separate CI from real-window evidence

**Gate M9:** a real board made from EAK outputs is inspected and measured, and discrepancies are
recorded and fixed. One board does not prove universal hardware correctness. Gate state: **blocked** (captain authorization).

### M10 — Product polish, installation, reliability and release
- [ ] M10.1 — End-to-end journeys for first run, project, agent, schematic, PCB, checks, manufacturing · `IN PROGRESS` · unassigned · deps: M2.1, M8.8
  - groundwork: Project Home and first run [#108](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/108) (`scenarios/g1-home-firstrun.json`), real-window PCB flow (`scenarios/p1-pcb-flow.json`)
- [ ] M10.2 — Every visible control functional, accessible and keyboard-operable · `IN PROGRESS` · unassigned · deps: M1.4
  - groundwork: dead-control sweep (`scenarios/a1-menus.json` e1 rows), honesty pass (`scenarios/f9-honesty.json`), keys (`scenarios/f8-keys.json`)
- [ ] M10.3 — App restart/recovery, crash handling, log/export and project backup/restore · `IN PROGRESS` · unassigned · deps: M3.5
  - groundwork: journal recovery, persisted UNKNOWN_OUTCOME recovery block
- [ ] M10.4 — Supported platforms verified on real CI desktop environments; platform limits documented · `IN PROGRESS` · unassigned · deps: none
  - groundwork: `ci` cross-platform jobs (Windows, macOS arm64/x64); ui-evidence is Linux/Xvfb only; `docs/PLATFORM_SUPPORT.md`
- [ ] M10.5 — Reliable install/update/uninstall, checksums, provenance and release automation · `IN PROGRESS` · unassigned · deps: M10.4
  - groundwork: `release.yml` (SHA256SUMS, provenance.json, installer smoke tests); [v0.2.0-rc.1](https://github.com/NijoP/electronics-agent-kit-new-prototype/releases/tag/v0.2.0-rc.1)
- [ ] M10.6 — Code-sign/notarize builds where credentials permit; disclose unsigned builds otherwise · `IN PROGRESS` · unassigned · deps: M10.5
  - limits: no signing credentials; releases are disclosed as unsigned pre-releases
- [ ] M10.7 — Performance/memory benchmarks on modest hardware; optimize measured bottlenecks · `NOT STARTED` · unassigned · deps: M7.12
  - groundwork: resource profiles and bounded jobs (code-reviewed only)
- [ ] M10.8 — Security review of secrets, provider login, MCP/tool execution, workspace access, agent isolation · `NOT STARTED` · unassigned · deps: M2.5, M3.6
- [ ] M10.9 — Documentation, tutorials, troubleshooting, architecture and feature status · `NOT STARTED` · unassigned · deps: Appendix B
- [ ] M10.10 — Release-candidate acceptance review and release decision · `NOT STARTED` · captain · deps: M10.1–M10.9

**Gate M10:** the release build is reproducible, verified, installable and documented, with no
high-severity issues or misleading status claims. Gate state: **open**.

### M11 — Finished-product acceptance and ongoing quality
- [ ] M11.1 — Full end-to-end product acceptance on representative projects and domains · `NOT STARTED` · unassigned · deps: Appendix A journeys
- [ ] M11.2 — Failure/recovery tests for agents, provider auth, task persistence and project writes · `NOT STARTED` · unassigned · deps: M3.5, journey F
- [ ] M11.3 — All engineering, DRC/ERC, routing, DFM and release gates on supported acceptance projects · `NOT STARTED` · unassigned · deps: M7.11, M8
- [ ] M11.4 — A precise feature matrix separating unit/CI/desktop/engineering/manufacturing/hardware evidence · `NOT STARTED` · unassigned · deps: M9.9
  - groundwork: `FEATURE_MATRIX.md` (older rows need a re-audit)
- [ ] M11.5 — Founder sign-off on product readiness and documented limitations · `NOT STARTED` · captain · deps: M11.1–M11.4

**Gate M11:** every roadmap-critical goal has `*` with evidence, unresolved limitations are
explicit, and the product completes its supported workflow reliably. Gate state: **open**.

---

## 6. Standing constraints for every goal
- One EAK-IR. No write path outside `Engine::execute`. Every IR change needs an ADR, a migration
  and a round-trip test, using the next free ADR and schema number at push time (now: ADR 0057,
  schema 9).
- The Terminal workspace stays functional and visually unchanged. `terminal.test.tsx` stays as it is.
- Every control does real work against real state. An unbuilt capability is shown as unavailable,
  never faked.
- Clean-room: never copy EasyEDA, Traycer, Cursor or Claude code, icons, artwork or text. No LCSC
  private APIs. No Python.
- All validation runs in GitHub Actions. No builds, tests or app runs on the founder's laptop.
- Acceptance tests are never weakened, and failures are never relabelled. Nothing is called
  manufacturing-ready.

---

## 7. Router and verification PR map (M0.6)

| PR | What | Owner | State | Roadmap goal |
|---|---|---|---|---|
| [#133](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/133) | coupled diff pairs, stack-up impedance, push-and-shove | electronics lead | merged [73be8a3](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/73be8a3); [ci 45841440634](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45841440634) success | M7.7 |
| [#134](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/134) | first verification engines (workstream B) | electronics lead | merged [c46646d](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/c46646d); [ci 45837718640](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45837718640) success | M8.1 |
| [#135](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/135) | multilayer autorouter on the real stack-up | electronics lead | merged [58f4fe3](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/58f4fe3); [ci 45832122842](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45832122842) success | M7.6 |
| [#136](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/136) | router determinism, acceptance evidence in CI output | electronics lead | merged [cea00f7](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/cea00f7); [ci 45847891125](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45847891125) success | M7.6, M7.12 |
| [#137](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/137) | diff-pair DRC-024..026, RF keep-out test | electronics lead | merged [969e3c5](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/969e3c5); [ci 45849973705](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45849973705) success | M7.8, M8.5 |
| [#138](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/138) | diff-pair length matching | electronics lead | merged [ef566c8](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/ef566c8); [ci 45844579169](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45844579169) success | M7.1 |
| [#139](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/139) | diff-pair push-and-shove | electronics lead | merged [a202556](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/a202556); [ci 45853087718](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45853087718) success | M7.1 |
| [#140](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/140) | blind/buried vias of an allowed span; per-net inflation retry | electronics lead | merged [500e938](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/500e938); [ci 45858355299](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858355299) success | M7.2, M7.5 |
| [#141](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/141) | via current DRC-027, junction temperature ERC-025, DRC-002 insulation | electronics lead | merged [0df008b](https://github.com/NijoP/electronics-agent-kit-new-prototype/commit/0df008b); [ci 45855454190](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45855454190) success | M7.3, M8.2–M8.4 |
| [#142](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/142) | complex constrained acceptance board | electronics lead | **open, red by design**: per-net clearance gap at the 0.8 mm ESP32-C3 pads, and the spacing pre-check ignored insulation/creepage. **Not to be weakened.** | M7.4, M7.5 |
| [#143](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/143) | DRC-028 DC voltage drop on a copper-network model | electronics lead | **open, CI red**: rule-ID registry test | M8.3 |
| no PR | diff-pair and RF keep-out checks (screenshot task) | electronics lead | landed in #137; reference-plane/return-path part still open | M7.8 |

---

## 8. Appendices

### Appendix A — Acceptance journeys (brief §14)
| Journey | Scope | State | Feeds |
|---|---|---|---|
| A — Embedded sensor board | MCU, battery/USB power, sensors, radio option, full flow to a draft package | NOT STARTED | M6.6, M9.5, M11.1 |
| B — Power electronics board | buck/boost or motor control; trace/via sizing, spacing, thermal, voltage drop, approvals | NOT STARTED (old [#10](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10) held) | M6.6, M8.3, M11.1 |
| C — RF/wireless board | antenna region, matching, keep-outs, honest unsupported EM states | NOT STARTED | M7.8, M11.1 |
| D — Dense multilayer diff-pair board | stack-up, constrained pads, pairs, skew, vias, keep-outs, per-net clearance | IN PROGRESS (#142) | M7.4, M7.11 |
| E — Multi-agent engineering task | scout/ship delegation, evidence, one proposal, approval, engine apply, re-check | NOT STARTED | M3, M11.1 |
| F — Recovery and audit | kill/restart agents or the app; ledger, status, proposal and revision reconcile | NOT STARTED | M3.5, M11.2 |

### Appendix B — Documentation deliverables (brief §16)
| ID | File | State | Goal |
|---|---|---|---|
| D1 | `ROADMAP_EXECUTION.md` | IN REVIEW | M0.2 |
| D2 | `docs/research/TRAYCER_REVERSE_ENGINEERING.md` | NOT STARTED | M1.1 |
| D3 | `docs/research/FIRSTMATE_REVERSE_ENGINEERING.md` | NOT STARTED | M1.2 |
| D4 | `docs/research/DESK_TO_EAK_DOMAIN_WORKFLOW.md` | IN PROGRESS (eak-desk-study) | M1.3 |
| D5 | `docs/product/EAK_PRODUCT_ARCHITECTURE.md` | NOT STARTED | M1.5 |
| D6 | `docs/product/EAK_AGENT_WORKSPACE_SPEC.md` | NOT STARTED | M1.5, M2.1 |
| D7 | `docs/architecture/EAK_AGENT_RUNTIME.md` | NOT STARTED | M2.9, M3 |
| D8 | `docs/architecture/EAK_ENGINEERING_KNOWLEDGE.md` | NOT STARTED | M4 |
| D9 | `docs/verification/EAK_ACCEPTANCE_MATRIX.md` | NOT STARTED | Appendix A, M11.4 |
| D10 | `STATUS.md`, `FEATURE_MATRIX.md`, release status, `README.md` kept to actual capability | IN PROGRESS (ongoing) | M10.9, M11.4 |

---

## 9. Change log (append-only, UTC)

- 2026-11-26T16:33Z — eak-roadmap — Roadmap created from the execution brief at `main` 500e938
  ([ci 45858355299](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/45858355299)
  success). Baseline audit recorded in §4. M0.1, M0.2, M0.5 and M0.6 set to IN REVIEW. Router PRs
  #133–#143 mapped in §7. M9.6 and M9.7 set to BLOCKED (awaiting captain authorization). No goal
  starred.
