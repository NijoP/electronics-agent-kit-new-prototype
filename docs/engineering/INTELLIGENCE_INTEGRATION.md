# Engineering intelligence integration inventory

Directive §9 inventory of what EAK recovers from the two earlier engineering repositories.
Current EAK stays authoritative: one EAK-IR, mutations only through `Engine::execute`, no Python.
Every EAK row marked *done* is **code-reviewed only** (never compiled or run).

## Sources (pinned)
| Repository | Revision | Licence | Read how |
|---|---|---|---|
| `NijoP/electronics-agent-kit-proto` (Rust prototype, same author) | `79f4e9d21f4d0b0577321de165270bebe8bb7db7` (2026-11-08) | no licence file; same owner, so EAK's `MIT OR Apache-2.0` applies to reused code, with attribution in the file header | shallow clone, targeted reads |
| `NijoP/DESK` (Python + EasyEDA Pro engineering system) | `5ad8492c37a5dac919d97f4eccd8250780490e7a` | MIT, © 2026 NijoP, plus `NOTICE` | shallow clone, targeted reads; **no DESK code copied** (EAK is Python-free); knowledge re-implemented in Rust |

Classes: REUSE (code copied with attribution) · ADAPT (code reshaped to EAK types) · TRANSLATE
(Python logic re-expressed in Rust) · REIMPLEMENT (independent implementation of the idea) ·
REFERENCE ONLY · REJECT. Risk: L/M/H (correctness or security impact if done wrong).

## Prototype repository (`electronics-agent-kit-proto`, Rust)
| Subsystem | Source files | EAK equivalent | Class | Data-model mapping | Licence / security / perf | Risk | Acceptance criteria | Owner / state |
|---|---|---|---|---|---|---|---|---|
| Physical quantities and units | `eak/crates/eak-units/src/lib.rs` (335 lines, serde only) | `eak-core::units` (new) | REUSE | `PhysicalQuantity{magnitude, unit, tolerance}`, SI normalisation; added `Micrometre`, `in_unit`, `temperature_difference_k` (a rise never gets the Celsius offset) | attribution header; no new deps; tiny | L | cross-dimension compare is an error; °C rise ≠ °C absolute; unit tests (written, unrun) | intel-core, **done** (`e0df591`); reused by eak-multidomain |
| Requirements and engineering entities (`DesignIntent`, `Requirement`, `Decision`, `Evidence`, `ProvenanceLink`, `Constraint`, `Violation`, `Waiver`, `Assumption`, `Risk`, `ModelFidelity`) | `eak/crates/eak-domain/src/lib.rs` (3.8k lines) | not in the IR; the harness design spec (ADR-0020) holds rails/conditions | ADAPT | onto EAK-IR ids and typed `Command`s; needs an IR schema change + ADR | serde only; must not become a second design model | M | entities live in the one EAK-IR; every mutation through a command; migration test | **eak-multidomain** (requirements / constraint graph / risk) |
| Component taxonomy and per-class metadata | `eak-domain` (`ComponentClass`, `*Metadata`, `Part`, `PartLifecycle`, `BomLineItem`) | `eak-core::library`, `eak-validation::sourcing` | REFERENCE ONLY (for now) | candidate library rating/lifecycle fields | — | L | taken up with component intelligence | eak-autodesign scope |
| Event-sourced runtime, state reconstruction | `eak-runtime`, `eak-store`, `eak-ports` | `eak-runtime` journal, deltas, recovery (ADR-0001/0003) | REJECT | — | would be a second persistence authority | H | — | — |
| Engineering phase state machines | `eak-phases` (5.4k lines) | harness design flow (ADR-0020) | REFERENCE ONLY | phase-gating ideas | — | M | — | eak-autodesign |
| Provider abstraction / reasoning | `eak-reasoning` (ureq) | `eak-intelligence` providers (keyring keys, redaction) | REJECT | — | duplicate provider stack | M | — | — |
| Datasheet parsing, asset library | `eak-assets` (8.6k lines; reqwest, rusqlite, chrono, regex, csv…) | datasheet hash/evidence in `sourcing` | REFERENCE ONLY | — | heavy dependency tree; document parsing is an attack surface | H | revisit with a sandboxed parser | — |
| KiCad import | `eak-kicad` | none (KiCad is not an engine or source of truth) | REFERENCE ONLY | possible one-way importer into EAK-IR | — | M | — | — |
| Engines / search, compiler | `eak-engines`, `eak-compiler` | `eak-engineering`, `eak-validation` | REFERENCE ONLY | — | rusqlite | M | — | — |
| Dashboard, observability, telemetry | `eak-dashboard`, `eak-observability`, `eak-telemetry` (axum, tokio, websockets) | none | REJECT | — | network services, heavy runtime (§5, §7) | H | — | — |
| Engineering-science knowledge base | `engineering-science/` (docs) | `docs/engineering/*` | REFERENCE ONLY | cite per formula when used | text only | L | each formula used cites its source | — |

## DESK (`NijoP/DESK`, Python; knowledge only)
| Subsystem | Source files | EAK equivalent | Class | Data-model mapping | Licence / security / perf | Risk | Acceptance criteria | Owner / state |
|---|---|---|---|---|---|---|---|---|
| Domain intelligence, board classification | `desk/domain_intelligence/*`, `library/pcb_domains/`, `.agents/skills/domain-intelligence/` | `eak-intelligence::domain` | TRANSLATE | archetype, overlays, rule provenance → EAK domain types | MIT; no runtime Python | H (wrong clearances) | missing critical info blocks automatic decisions | **eak-autodesign** (3A) |
| Schematic engineering procedure | `.agents/skills/schematic-engineering/`, `schematic_io.py`, `netcheck.py` | harness design flow, `eak-engineering::netlist`, ERC | TRANSLATE | blocks → typed `Batch` commands; read-back via netlist | — | H | pin maps never invented; read-back compare | **eak-autodesign** (3B) |
| Component intelligence | `desk/component_intelligence/*`, `tools/resolve.py`, `tools/pins.py` | `sourcing`, library, LCSC lookup | TRANSLATE | MPN, datasheet evidence, derating | supplier HTTP read-only, rustls | M | every selection carries evidence | **eak-autodesign** (3C) |
| Placement intelligence | `placement.py`, `placement_intel.py`, `placement_refine.py`, `tools/pcb_place.py` | `eak-engineering::{placement_intel, placer}` | TRANSLATE | roles, snaps, measurable constraints | — | M | placement reports measurable reasoning | **eak-desk-placement** (3D), landed `f9ea4bc` |
| Routing intelligence | `desk/routing/*`, `router.py`, `routability.py` | `eak-engineering::{priority, router, interactive}` | TRANSLATE | net priority, order, passes | Freerouting/DSN: REJECT (never mandatory) | M | correctness before completion | **eak-pcb-routing** (3E), landed `80820e5` |
| IPC sizing tables | `desk/ipc.py` | `eak-engineering::ipc` | REIMPLEMENT (earlier) | trace/via ampacity, Table 6-1, creepage | `VERIFIED_AGAINST_STANDARD = false` | M | DESK values as regression tests | done before this work |
| Copper and power distribution | `desk/copper.py`, `desk/power_distribution.py`, `desk/profile.py` | `eak-engineering::power` (new); `copper` (fill, islands, reliefs) | REIMPLEMENT | typed `TraceWidthInput` / `ConductorInput`; rail drop from IR tracks + declared currents; return-path gaps over adjacent ground fill; plane splits | DESK silently defaults 1 oz / 10 °C — **not carried over**: every conclusion-changing input is required | M | explicit inputs; source + applicability on each result; unknown never a pass | intel-core, **done** (§3F, `e0df591`) |
| Zone planning, stitching-via synthesis, zone revision tracker | `copper.py` (`plan_power_polygons`, `synthesize_stitching_vias`, `CopperZoneRevisionTracker`) | none | REFERENCE ONLY | needs keepout objects in the IR (ADR) | — | M | — | later |
| Quality gates (fail-closed lifecycle) | `desk/quality.py`, `tools/verify.py` | `eak-validation::gates` (new) over DRC, DFM, stackup, netlist, power | REIMPLEMENT | `GateState` PASS/FAIL/UNKNOWN/NOT_RUN; stale via `is_current_for`; evidence = report, finding ids, revision, input hash | weighted score **rejected** (a score must never override a gate) | H | unknown never passes; failed/unknown required gates keep packages DRAFT | intel-core, **done** (§3G, `0739f28`); ADR-0021 |
| DRC / ERC engines | `drc_engine.py`, `erc.py`, `netcheck.py` | `eak-validation::{drc, erc}` | REFERENCE ONLY | — | — | L | — | — |
| Manufacturing / JLCPCB | `desk/manufacturing/jlcpcb`, `release.py`, `ship.py` | `eak-manufacturing` | REFERENCE ONLY | — | no ordering or uploading | L | — | — |
| Rework and recovery | `desk/rework/*` (baseline, planner, readiness, executor) | none | REIMPLEMENT (later) | baseline = checkpoint; repair plan = typed `Batch`; approval policy | — | H | read-only first; immutable baseline; readback | **out of scope now (3H)**; see `docs/development/NEXT_ACTION.md` |
| Credential storage | `desk/auth/storage.py` | OS keyring (`apps/desktop/src-tauri/src/models.rs`) | REJECT | — | reversible base64/XOR obfuscation of secrets in a file — must not be inherited | H | keys only in the OS credential store | — |
| Local services, EasyEDA bridge | `desk/runtime/server.py`, `desk/weft/service.py`, `bridge.py` | token-authenticated harness endpoint, terminal API | REJECT | — | network services / EasyEDA as engine are out of policy | H | — | — |

## Harness exposure (this work)
`run_gates` (validation; stored as `GATES` evidence), `calc_copper_trace_width`,
`calc_copper_voltage_drop` and `power_analysis` (read). `calc_trace_width` no longer falls back
to 1 oz silently.
