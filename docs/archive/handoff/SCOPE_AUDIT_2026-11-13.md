# EAK architecture and scope audit — general-purpose platform vs. two acceptance projects

Task `eak-scope-audit` · scout (read-only, no PR) · audited 2026-11-13, 18:40–19:05 IST

## 0. What was audited, and what was not

| Item | Evidence |
|---|---|
| `origin/main` | `bedd9b5` after `git fetch origin --prune`; 67 tracked files, 9,295 lines of Rust/docs |
| Unmerged branches | all 12 `origin/fm/*` branches = all 12 open pull requests (table below); each is exactly `main` + 1–3 commits |
| Tests run | `CARGO_BUILD_JOBS=2 cargo test --workspace --release` on `main`: **35 passed, 0 failed** (eak-core 7, eak-engineering 11, eak-manufacturing 3, eak-runtime 10 + 4; **eak-validation has 0 tests of its own**). rustc 1.99.1. Branch test suites were **not** run (read only). |
| DESK | shallow clone of `NijoP/DESK` at `5ad8492`, read only, no Python executed. A sub-agent surveyed it; I re-checked licence, state names, archetype/rule status counts and the composer myself. |
| Source of truth | the three PDFs in `docs/source-of-truth/`, text-extracted with `pdftotext` |
| **Not visible** | **Unpushed local branches of the four teams. Nothing in this report describes them.** Any work done today that is not in the 12 branches below is unaudited. |

Branch → pull request (referred to by branch name below):

| Branch | Pull request | CI on head |
|---|---|---|
| `fm/eak-dev-method` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/1 | — |
| `fm/eak-scripts-exec` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/2 | success |
| `fm/eak-intelligence-core` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/3 | 1 pass, 2 fail |
| `fm/eak-stackup` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/4 | 1 pass, 2 fail |
| `fm/eak-fab-detail` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/5 | 1 pass, 2 fail |
| `fm/eak-power-calc` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/6 | Windows pass; Ubuntu and macOS fail |
| `fm/eak-lib-expand` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/7 | 1 pass, 2 fail |
| `fm/eak-distributor-sourcing` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/8 | 1 pass, 2 fail |
| `fm/eak-val-esp32` (draft) | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/9 | 1 pass, 2 fail |
| `fm/eak-val-power` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10 | 1 pass, 2 fail |
| `fm/eak-cli-ci` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/11 | 4 of 4 pass |
| `fm/eak-desktop-shell` | https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/12 | in progress (3 pass, 0 fail of 8) |

Paths without a branch prefix are on `main`. `branch:path:line` means the file on that branch.

## Bottom line

1. **`main` is not built around ESP32 or the buck converter.** There is not one ESP32/buck token in `crates/` on `main`. The reference fixture is an ATtiny85 + AMS1117 board.
2. **`main` is not general-purpose either. It is a domain-blind, two-layer, low-frequency core.** It has no concept of domain, archetype, component family, role, design profile, keep-out, impedance or isolation, and no requirements engine. Every design gets the same 29 generic rules, and an RF or mains design is not flagged as outside what EAK can verify.
3. **The narrowing is happening in the unmerged branches**, where the two acceptance boards are starting to define the library and the calculation API (`fm/eak-lib-expand`, `fm/eak-power-calc`).
4. **The two validation boards pass largely by hand**, not by engine capability: hand placement, hand-drawn critical routes, antenna keep-out by board overhang, no impedance. Both projects say so honestly in their own gap lists.
5. Stack compliance is good: no Python on any branch, one typed command path, native Rust algorithms. Gaps are that Tauri/React/Vite exist only in an open pull request, Three.js does not exist anywhere, and Linux/macOS CI fails on `main`.

---

## 1. The seven audit questions

### 1.1 Has the architecture been designed around only the ESP32 and buck-converter use cases?

**No on `main`; partly yes in two open branches.**

- `main` has no project-specific code. A search of `crates/` for `esp32|buck|dashboard|sht4|stm32|rp2040` returns nothing. The only named parts are AMS1117 (`crates/eak-core/src/builtin.rs:287-326`) and ATtiny85, used by the reference fixture (`crates/eak-runtime/src/fixtures.rs:4,77`).
- The command set is generic create/move/delete plus `AutoPlace`/`AutoRoute` (`crates/eak-runtime/src/command.rs:14-190`).
- `fm/eak-lib-expand` adds a function to the **core** crate whose stated scope is the two boards: "Parts with manufacturer pinouts and land patterns, for the ESP32-C3 dashboard board and the 12-24 V to 5 V buck board" (`fm/eak-lib-expand:crates/eak-core/src/builtin.rs:808-810`).
- `fm/eak-power-calc` adds switching-converter maths as `buck_*` functions only (`fm/eak-power-calc:crates/eak-engineering/src/calc.rs:149-352`).
- The structural ceiling is two copper layers, hard-wired in five places: `crates/eak-core/src/ir.rs:112-117`, `crates/eak-runtime/src/command.rs:722-726`, `crates/eak-engineering/src/router.rs:50,98-111`, `crates/eak-manufacturing/src/lib.rs:164-189`, and still enforced by the new v2 stackup (`fm/eak-stackup:crates/eak-core/src/ir.rs:221-226`). Both acceptance boards fit under that ceiling, so neither forces it to move.

### 1.2 Are the engineering algorithms generic, or hardcoded for these two projects?

**Generic in geometry, but with one fixed behaviour for every design, and thin in electrical content.**

- Placer: orders parts by net degree and minimises distance to connected parts (`crates/eak-engineering/src/placer.rs:80-85,108-127`). No grouping, priorities or keep-outs. Grid 0.5 mm and spacing 0.3 mm are literals at the call site (`crates/eak-runtime/src/command.rs:954`).
- Router: two-layer A* over a net minimum spanning tree (`router.rs:132`). Every obstacle is inflated by the **worst-case** class clearance plus half the **widest** track (`router.rs:167-179`); pitch 0.25 mm and via cost 40 are fixed defaults (`router.rs:121-129`). The ESP32 project reports this as the reason it "cannot leave 0.5 mm-pitch lands" (`fm/eak-val-esp32:validation/esp32-dashboard/READINESS.md:80`).
- Calculations on `main` are six textbook functions: LED resistor, resistor power, divider, linear-regulator dissipation, junction temperature, RC constant (`crates/eak-engineering/src/calc.rs:58-106`).
- IPC-2221 width, clearance, IEC 60664-1 creepage and via current are generic but declared unverified: `VERIFIED_AGAINST_STANDARD: bool = false` (`crates/eak-engineering/src/ipc.rs:14`).
- DRC always sizes for an external conductor (`crates/eak-validation/src/drc.rs:61`) and IPC condition B2 (`drc.rs:168,196-199`).
- In `fm/eak-power-calc` the only switching topology is buck, and buck assumptions leak into generic rules: the inductor rule's rationale is "peak current is 1.2x DC at a 40 % ripple ratio" (`fm/eak-power-calc:crates/eak-validation/src/erc.rs:442`), and every FET is judged against "switch-node ringing" (`:482`), which is the wrong reasoning for a load switch or a linear pass device.

### 1.3 Can the EAK-IR express different electronics topologies and technologies?

**Any flat circuit topology, yes. Most technologies, no.**

Schematic connectivity is component-agnostic (`crates/eak-core/src/ir.rs:47-56`), so any circuit can be captured. What the IR cannot express:

| Missing in EAK-IR | Evidence | Blocks |
|---|---|---|
| Inner copper layers | `CopperLayer { Top, Bottom }` (`ir.rs:112-117`) | high-speed, RF stripline, power planes |
| Dielectric constant, per-layer build-up | `Stackup` has count, thickness, outer oz, name (`ir.rs:146-153`). `fm/eak-stackup` adds per-layer εr and thickness but still exactly two layers | impedance |
| Via layer span | `Via` has position, drill, diameter, net (`ir.rs:200-207`) | blind/buried, via fencing by layer |
| Keep-outs, cut-outs, slots | no such type; `grep keepout\|cutout` in `crates/` is empty | antenna keep-out, isolation slots, mains |
| Impedance, differential pair, length, net role | `NetClass` is width, clearance, via size (`ir.rs:228-237`) | RF, USB, Ethernet, DDR |
| Frequency, isolation groups, pollution degree, safety class | `Constraints` carries only per-net current and voltage (`ir.rs:249-254`) | HV, mains, EMC, switching |
| Hierarchy and buses | "hierarchy not yet supported" (`crates/eak-validation/src/erc.rs:377`) | FPGA and processor carriers |
| Component family and role | `category: String`, free text (`crates/eak-core/src/library.rs:105`); `Rating.name: String` (`library.rs:82-90`) | every domain rule |
| Structured requirements | `Requirement { id, text }` (`ir.rs:41-45`) | classification |
| Domain, archetype, profile | no field anywhere | everything in this directive |
| Pad shapes beyond rectangle, circle, oval | `PadShape` (`library.rs:41-47`) | QFN/BGA, RF connectors |

### 1.4 Can the requirements engine classify unfamiliar or mixed-domain projects?

**There is no requirements engine.**

- A requirement is an id and a free-text string (`ir.rs:41-45`). `AddRequirement` only appends it (`crates/eak-runtime/src/command.rs:364-371`).
- Nothing reads requirements. The only references in `crates/` are the field, its default, the append, and a delta section name (`ir.rs:30,335`; `command.rs:366`; `delta.rs:25`).
- No classifier exists on `main` or any pushed branch. In `fm/eak-intelligence-core` the word "domain" appears only as `tools::Domain`, an enum of command families (Project, Library, Schematic, Pcb, Constraints, Manufacturing) at `fm/eak-intelligence-core:crates/eak-intelligence/src/tools/commands.rs:19-26`.
- The agent system prompt is generic and has no requirement-interpretation step (`fm/eak-intelligence-core:crates/eak-intelligence/src/agent.rs:219-233`).
- Consequence: an unfamiliar project is neither classified nor flagged. ERC and DRC can return `NO_ERRORS_IN_IMPLEMENTED_CHECKS` (`crates/eak-validation/src/report.rs:39-43,100-104`) with a `not_implemented` list that is a fixed vector, identical for every design (`erc.rs:375-380`; `drc.rs:464-473`). A 2.4 GHz or a 230 V design gets the same list as an LED blinker.

### 1.5 Can new domain rules and component families be added without rewriting the core?

**Component definitions yes; families and rules no.**

- Definitions are data: `UpsertComponentDefinition` (`command.rs:26`) is how both validation projects add their parts (`fm/eak-val-power:validation/power-buck/src/design.rs:313`; `fm/eak-val-esp32:validation/esp32-dashboard/design/src/lib.rs:167`).
- Families are not modelled. Rules match literal category strings in Rust: `"Capacitor"` and `"Resistor"` (`erc.rs:56,262,310`), plus `"Inductor"` and `"Transistor"` in `fm/eak-power-calc:crates/eak-validation/src/erc.rs:392,432,467`. A part with category `"MLCC"` silently escapes the capacitor checks.
- Rules are inline blocks inside one function per check (`erc::run`, `erc.rs:40-386`). The rule catalogue is a hand-built vector (`erc.rs:325-374`; `drc.rs:450-462`). There is no rule type, registry or parameter data; margins such as 1.5× are literals (`erc.rs:294`).
- The built-in library is compiled Rust (`builtin.rs`), not data.
- Adding a domain rule today means editing `eak-validation` source.

### 1.6 Are routing, placement, copper, verification and manufacturing domain-independent where appropriate?

**Domain-independent, yes. Domain-aware where they need to be, no.**

| Engine | State on `main` | Evidence |
|---|---|---|
| Routing | no net priority, no differential pairs, no length or impedance, two layers | `router.rs:50,121-129,167-179` |
| Placement | connectivity only; no functional groups, no thermal, RF or HV separation | `placer.rs:80-127` |
| Copper | zone fill on a 0.1 mm raster; no stitching or via fencing; thermal reliefs only in `fm/eak-fab-detail` | `crates/eak-engineering/src/copper.rs:283-285` |
| Verification | 13 ERC + 13 DRC + 3 MECH generic rules; unsupported list is static | `erc.rs:325-380`; `drc.rs:450-477` |
| Manufacturing | fixed top/bottom layer set; `fabricator_profile` is stored but never read | `crates/eak-manufacturing/src/lib.rs:164-189`; `ir.rs:299,309` are its only uses |
| Release gate | needs ERC, DRC, MECH with zero errors; knows nothing of domain coverage | `crates/eak-manufacturing/src/lib.rs:239` |

`ipc::creepage_mm` exists (`ipc.rs:173`) but no DRC rule calls it, so creepage is computed nowhere in a check.

### 1.7 Are any agents implementing project-specific shortcuts?

From pushed branches only:

| Branch | Finding | Evidence |
|---|---|---|
| `fm/eak-lib-expand` | Core library seeded, in compiled Rust, with parts chosen for the two boards | `builtin.rs:808-810`; ESP32-C3-MINI-1 `:899`, LMR33630 `:958`, SHT40 `:1006`, AO4406A `:1061` |
| `fm/eak-lib-expand` vs. the boards | Those parts are not the ones the boards use. The buck board uses TPS54302 (`fm/eak-val-power:validation/power-buck/DESIGN.md:25`); the ESP32 board uses ESP32-C3-WROOM-02 (`fm/eak-val-esp32:validation/esp32-dashboard/design/src/parts.rs:63,128`) | three teams defined the same boards' parts three ways |
| `fm/eak-power-calc` | `buck_*` API where the buck project itself asked for a topology-parameterised one (`Topology::{Buck, Boost, BuckBoost, ..}`) | `calc.rs:149-352` vs. `fm/eak-val-power:validation/power-buck/GAPS.md` row G1 |
| `fm/eak-power-calc` | Buck rationale inside generic inductor and FET rules | `erc.rs:442,482` |
| `fm/eak-val-power` | No `AutoPlace` or `AutoRoute` call at all: every footprint is moved and every track drawn by the program | `design.rs:417,434,444,468`; no auto command in `validation/power-buck/src` |
| `fm/eak-val-esp32` | `AutoRoute` for most nets, but the USB-C escape and D±/CC/VBUS runs are hand-drawn; antenna keep-out "handled by overhang only"; D± has no impedance control; USB shell slots emitted as round holes, so the package is "FAIL / NOT VERIFIED" as fabricable | `lib.rs:136,390,427`; `READINESS.md:35,41,44,46,84` |
| `fm/eak-intelligence-core` | None found. Prompt and tools are generic; the calculation tools are the six `main` functions | `agent.rs:219-233`; `tools/readonly.rs:67-141` |
| `fm/eak-cli-ci`, `fm/eak-desktop-shell` | None found. `demo` builds the ATtiny fixture | `apps/cli/src/main.rs:472-477` |

The two validation projects behave correctly as acceptance tests: they are stand-alone crates outside the workspace, drive only `Engine::execute`, and list what EAK could not do rather than faking it. The risk is in how their results are read: a passing board shows the command engine works, not that the placement, routing or RF engines can do the job.

---

## 2. Hardcoded assumptions, limitations, missing subsystems, use-case-specific code

Severity: **H** blocks general-purpose use or can mislead; **M** limits a domain family; **L** tidy-up. Owner is the owning domain.

### 2.1 Missing subsystems

| # | Missing | Evidence | Sev | Owner |
|---|---|---|---|---|
| S1 | Requirements interpretation: structured facts, units, tolerances, unresolved items | `ir.rs:41-45`; nothing reads `requirements` | H | eak-core (IR), eak-intelligence (extraction) |
| S2 | Domain and archetype classification, including "unknown" | no code on any branch | H | eak-electronics |
| S3 | Domain knowledge packs, rule composition, conflict reporting | no code on any branch | H | eak-electronics |
| S4 | Component families and roles | `library.rs:105` | H | eak-core (field), eak-electronics (vocabulary) |
| S5 | Rule registry: rules as data with pluggable evaluators | `erc.rs:40-386`, `drc.rs` are monolithic | H | eak-electronics |
| S6 | Domain coverage in reports and in the release gate | `report.rs:46-61`; `eak-manufacturing/src/lib.rs:239` | H | eak-electronics |
| S7 | RF and signal integrity: impedance, differential pairs, return path, via fencing | `drc.rs:465-466` lists them as not implemented | H | eak-electronics |
| S8 | Keep-out, cut-out and slot objects | none in `crates/`; `READINESS.md:44,46` | H | eak-core, eak-electronics |
| S9 | Multi-layer boards: inner layers, via spans, planes | `ir.rs:112-117`; `command.rs:722-726` | H | eak-core, eak-electronics |
| S10 | Creepage and isolation checks | `ipc.rs:173` is not called by DRC | H | eak-electronics |
| S11 | Thermal model: copper spreading, zone current density | `fm/eak-val-power:validation/power-buck/GAPS.md` G3 | M | eak-electronics |
| S12 | Fabricator capability profiles | `ir.rs:299` is never read | M | eak-electronics |
| S13 | Hierarchical sheets and buses | `erc.rs:377` | M | eak-core |
| S14 | Maturity and evidence state model per domain | none; `STATUS.md` is prose | H | eak-electronics |
| S15 | Domain view in the desktop app: profile, coverage, conflicts, matrix | `fm/eak-desktop-shell` has no such panel | M | eak-desktop |
| S16 | Agent tools for classification and coverage; rule to state missing knowledge | `fm/eak-intelligence-core:…/agent.rs:219-233` | M | eak-intelligence |
| S17 | 3D presentation layer | no `three` or R3F in `fm/eak-desktop-shell:apps/desktop/package.json` | M | eak-desktop |
| S18 | Unit tests for ERC, DRC and MECH | `cargo test`: eak-validation 0 tests on `main` | M | eak-electronics |

### 2.2 Hardcoded assumptions and architectural limitations

| # | Assumption | Evidence | Sev | Owner |
|---|---|---|---|---|
| A1 | Exactly two copper layers | `ir.rs:112-127`; `command.rs:722-726`; `router.rs:50,98-111`; `eak-manufacturing/src/lib.rs:164-189`; `fm/eak-stackup:…/ir.rs:221-226` | H | eak-core |
| A2 | Rules select parts by literal category string | `erc.rs:56,262,310` | H | eak-electronics |
| A3 | Supply and ground detected by pin and net name lists | `erc.rs:8-38` | M | eak-electronics |
| A4 | `not_implemented` and `limitations` are fixed text, not derived from the design | `erc.rs:375-384`; `drc.rs:464-477` | H | eak-electronics |
| A5 | A clean report says `NO_ERRORS_IN_IMPLEMENTED_CHECKS` whether or not the implemented checks are relevant to the design | `report.rs:39-43,100-104` | H | eak-electronics |
| A6 | Router inflates all copper by the worst-case clearance and widest track | `router.rs:167-179` | H | eak-electronics |
| A7 | Router pitch, via cost and expansion limit fixed; not reachable from the command | `router.rs:121-129`; `command.rs:977-983` | M | eak-electronics |
| A8 | Placer grid and spacing literals; rotations limited to 0° and 90° | `command.rs:954`; `placer.rs:95` | M | eak-electronics |
| A9 | Conductor always external; clearance condition always B2 | `drc.rs:61,168,196-199` | M | eak-electronics |
| A10 | Stackup default FR-4, 1.6 mm, 1 oz; εr fixed 4.5 in `fm/eak-stackup` | `ir.rs:155-163`; `fm/eak-stackup:…/ir.rs:149` | M | eak-core |
| A11 | Design-rule defaults sized for "common low-cost 2-layer fabrication" | `ir.rs:262-285` | L | eak-core |
| A12 | IPC and IEC tables transcribed, unverified against the standards | `ipc.rs:8-14` | H | eak-electronics |
| A13 | Creepage refuses pollution degree 3 and above, and anything over 1000 V | `ipc.rs:179-200` | M | eak-electronics |
| A14 | Capacitor margin 1.5× as a literal; other margins literal in `fm/eak-power-calc` | `erc.rs:294`; `fm/eak-power-calc:…/erc.rs:392-490` | M | eak-electronics |
| A15 | Built-in library is compiled code | `builtin.rs:124` | M | eak-core |
| A16 | Desktop wire types are a hand-written copy of the Rust types | `fm/eak-desktop-shell:apps/desktop/src/types.ts:1-25` (`CopperLayer = "top" \| "bottom"`) | M | eak-desktop |
| A17 | Auto-routed copper is not reproducible between builds (random id order) | `fm/eak-val-esp32:validation/esp32-dashboard/READINESS.md:23` | M | eak-electronics |
| A18 | `tools::Domain` means "command family"; it will collide with electronics domain | `fm/eak-intelligence-core:…/tools/commands.rs:19` | L | eak-intelligence |

### 2.3 Use-case-specific implementations

| # | Item | Evidence | Sev | Owner |
|---|---|---|---|---|
| U1 | `board_parts()` for the two boards inside `eak-core` | `fm/eak-lib-expand:crates/eak-core/src/builtin.rs:808-810` | M | eak-electronics |
| U2 | `buck_*` calculation API, no topology parameter | `fm/eak-power-calc:crates/eak-engineering/src/calc.rs:149-352` | M | eak-electronics |
| U3 | Buck rationale in generic inductor rule | `fm/eak-power-calc:crates/eak-validation/src/erc.rs:442` | M | eak-electronics |
| U4 | Switching-converter rationale applied to every FET | `fm/eak-power-calc:crates/eak-validation/src/erc.rs:467-482` | M | eak-electronics |
| U5 | Three unrelated definitions of the acceptance boards' parts | see 1.7 | M | eak-electronics |
| U6 | Reference fixture is one fixed board in the runtime crate | `crates/eak-runtime/src/fixtures.rs:4-184` | L | eak-core |
| U7 | Non-Rust inspection tooling in a validation project (awk, shell) | `fm/eak-val-power:validation/power-buck/tools/` | L | eak-electronics |

---

## 3. DESK lessons

DESK at `5ad8492`. Items marked ✓ I re-checked in the clone; the rest come from the sub-agent's reading of the files it lists as read in full (`classifier.py`, `rule_composer.py`, `profile_compiler.py`, `provenance.py`, `models.py`, `rule_engine.py`, `ipc.py`, `routing/strategy.py`, the schemas and the evidence policy).

### 3.1 Licence

**MIT** ✓ (`LICENSE`: "MIT License, Copyright (c) 2026 NijoP"; `pyproject.toml:11`). EAK's own research note records the same (`docs/REPOSITORY_RESEARCH.md:11`). Re-implementing its knowledge in Rust is permitted; if any DESK text or JSON is copied verbatim, the MIT notice must travel with it. DESK's `NOTICE` lists Freerouting (GPL-3.0) and Weft (non-OSI) as tools it calls but does not ship; neither is relevant to a Rust re-implementation.

### 3.2 The separation model

- **Taxonomy → archetype.** 14 domains, 100 archetypes ✓, one JSON file each. An archetype holds prose engineering guidance plus two machine-readable parts: classification hints and a maturity record with rule ids.
- **Overlays are cross-cutting.** 17 overlays ✓, each with an `applies_when` condition and a numeric precedence. Mains safety isolation ranks highest (100), low power lowest (30).
- **Families are not parts.** 206 component families in 9 groups ✓, kept apart from real-part identity, verification and sourcing. A "role" is a family id that an archetype requires.
- **Rules are data.** 66 rules in 10 sets ✓, each with scope, severity, parameters, source reference, status and an enforcement mode. Evaluators are a registry in code.
- **The profile carries the numbers.** A compiled design profile holds layers, stackup, rails, currents, net classes, differential pairs, RF lines, clearance condition and pollution degree. Engines read numbers only from the profile; the domain record is informational.
- **Evidence states are separate from enforcement.** Eight states ✓: DRAFT, ADVISORY, IMPLEMENTED, OFFLINE_TESTED, VERIFIED_AGAINST_SOURCE, LIVE_VERIFIED, NEEDS_HARDWARE_VALIDATION, BLOCKED. Only a human may grant the two verified states, and a rule may be ENFORCED only if a real evaluator exists ✓ (`docs/domain_intelligence/EVIDENCE_AND_MATURITY_POLICY.md:11-21`).
- **Unknown fails closed.** A project that matches no archetype is UNCLASSIFIED and the compiled profile is blocked with a missing-information list ✓ (`profile_compiler.py:70`).

### 3.3 What DESK's own labels say

| Item | DESK state |
|---|---|
| Archetypes | 87 DRAFT, 13 OFFLINE_TESTED ✓; the 13 are tested against synthetic fixtures |
| Rules | 38 OFFLINE_TESTED, 19 ADVISORY, 9 IMPLEMENTED ✓ |
| Anything VERIFIED_AGAINST_SOURCE or LIVE_VERIFIED | **none** |
| Standards references | all 46 are REFERENCE_ONLY ✓ |
| IPC and IEC tables | `verified_against_standard = False` ✓ (`desk/ipc.py:37`) |
| Impedance formulas (microstrip, stripline, differential) | **absent** ✓ (my search); the sub-agent reports DESK's own roadmap item as open and its capability matrix as MISSING |
| `desk/placement/` | does not exist ✓; placement is in flat files |

So DESK is a source of structure and of unverified candidate values. It is not a source of validated engineering.

### 3.4 What not to copy

| DESK behaviour | Problem | EAK design answer |
|---|---|---|
| One primary archetype; CLASSIFIED needs a 3-point lead over the runner-up ✓ (`classifier.py:262-269`) | a genuinely mixed board drops to REVIEW_REQUIRED as "close candidates" | return a set of archetypes (4.5) |
| Keyword scoring; 44 of 100 archetypes have no feature rules ✓ | prose decides the domain | hard signals from the IR; keywords capped (4.5) |
| Strictness inferred from parameter-name prefixes ✓ (`rule_composer.py:19-20`) | a misnamed parameter composes the wrong way | explicit `bound` per constraint (4.4) |
| Archetype engineering content is prose | not checkable | every rule names an evaluator or a `needs` (4.4) |
| Routing strategy chosen from net properties, never from the domain | domain knowledge does not reach the router | `Strategies` passed to placer and router (4.10 step 6) |

### 3.5 Knowledge worth re-implementing in Rust first

| # | Knowledge | DESK location | DESK label | EAK today |
|---|---|---|---|---|
| 1 | Fail-closed on undeclared current or voltage for power and HV nets | `profile.py`, rules ELEC-UNKNOWN-002, MAINS-DECL-160 | OFFLINE_TESTED | not present; DRC-008 checks only nets that declare a current |
| 2 | Creepage check, reinforced insulation ×2 | `ipc.py:103-133`, ELEC-CREEP-004 | OFFLINE_TESTED, "table transcription unverified" | function ported (`ipc.rs:173`), no rule uses it |
| 3 | Antenna keep-out, no copper in keep-out | PLACE-ANT-014, COPPER-ANT-045 ✓ | OFFLINE_TESTED, ENFORCED; value is a human datasheet input | needs keep-out in the IR |
| 4 | USB-C CC resistor check (sink 5.1 kΩ, source 56k/22k/10k) | `rule_engine.py:301-318`, USBC-CC-150 | OFFLINE_TESTED | not present; relevant to the ESP32 board |
| 5 | Inductor saturation margin including boost input current | `rule_engine.py:235-248`, DCDC-ISAT-120 | OFFLINE_TESTED | buck only, in `fm/eak-power-calc` |
| 6 | Rail strategy and voltage-drop budget (plane, polygon, wide trace) | `power_distribution.py:136-175`, ELEC-DROP-006 | OFFLINE_TESTED | not present |
| 7 | Charge voltage by chemistry and cell count, fail-closed | `rule_engine.py:262-275`, BAT-CHEM-130 | OFFLINE_TESTED | not present |
| 8 | LoRa regional band table | LORA-BAND-110 ✓ | OFFLINE_TESTED | not present; check against the regulators' texts |
| 9 | RF harmonic coincidence check | EMI-RFHARM-064 ✓ | OFFLINE_TESTED, **no reference cited** | treat as an unverified idea |
| 10 | 50 Ω feed with ground fence | ROUTE-RF-022 ✓ | IMPLEMENTED, ADVISORY only | not present; no impedance maths in DESK to port |
| — | Trace ampacity, clearance table, via current | `ipc.py` | OFFLINE_TESTED, unverified | already ported (`ipc.rs`), still unverified |

Every row keeps DESK's label as the ceiling. In EAK each starts at `DRAFT` and rises only by the rules in 4.8.

---

## 4. Minimal Rust design for domain knowledge packs

### 4.1 Decisions

| Question | Decision | Why |
|---|---|---|
| Where does it live? | One new crate, `crates/eak-knowledge`, depending only on `eak-core`, `serde`, `serde_json` | Packs are data plus pure functions needed by validation, engineering, runtime, intelligence and desktop. A leaf crate avoids cycles. |
| Pack format | Versioned JSON, embedded with `include_str!` and also loadable from a folder | `serde_json` is already a workspace dependency. TOML would add a crate. **No new dependencies.** |
| Change to EAK-IR | One optional `domain_profile` on `Project`, optional `facts` on `Requirement`, optional `family` on `ComponentDef`, optional `role` on `SymbolInstance`. All `#[serde(default)]`. | Additive; old projects load unchanged. Still one canonical IR. |
| Change to commands | Three commands: `SetRequirementFacts`, `SetDomainProfile`, `SetSymbolRole`. Family goes through the existing `UpsertComponentDefinition`. | Every mutation still goes through `Engine::execute`. |
| Who classifies? | A deterministic Rust function proposes; a human or approved agent commits with `SetDomainProfile` | The agent never self-certifies. The model may only propose structured facts. |
| Rules | Data in packs, each naming a Rust evaluator in `eak-validation` | A rule with no evaluator in this build reports `NOT_IMPLEMENTED`, never a pass. |
| Maturity | Computed by Rust from evidence, capped by what the pack declares | A JSON file alone can never score above `DRAFT`. |

This is a shared-contract change (IR types, `Command`, report types), so `AGENTS.md` requires an ADR first: **ADR-0009 "Domain knowledge packs and domain profile"**.

Crate dependency order after the change (no cycles):

```
eak-core  <-  eak-knowledge  <-  eak-engineering  <-  eak-validation  <-  eak-manufacturing
    ^               ^                  ^                    ^
    +---------------+------------------+--- eak-runtime     +--- eak-intelligence, apps/desktop, apps/cli
```

### 4.2 EAK-IR additions (`eak-core`)

```rust
// ir.rs
pub struct Project { /* existing */  #[serde(default)] pub domain_profile: Option<DomainProfile> }

pub struct Requirement { pub id: Id, pub text: String, #[serde(default)] pub facts: Vec<RequirementFact> }

pub struct RequirementFact {
    pub key: String,          // controlled vocabulary, e.g. "vin.max_v", "rf.band_hz", "isolation.required"
    pub value: FactValue,     // Number(f64) | Range{min,max} | Text(String) | Flag(bool)
    pub unit: Option<String>,
    pub origin: FactOrigin,   // User | AgentProposed | Derived
    pub confirmed: bool,      // only confirmed facts count as hard signals
}

pub struct DomainProfile {
    pub archetypes: Vec<PackRef>,       // several allowed
    pub overlays: Vec<PackRef>,
    pub state: ProfileState,            // Declared | ReviewRequired | Unclassified
    pub resolutions: Vec<RuleResolution>, // explicit conflict decisions, with actor and rationale
    pub declared_by: String,
}
pub struct PackRef { pub pack: String, pub version: String, pub id: String }
pub struct RuleResolution { pub param: String, pub target: String, pub chosen: f64,
                            pub rationale: String, pub actor: String, pub revision: u64 }

// library.rs
pub struct ComponentDef { /* existing */ #[serde(default)] pub family: Option<String> }  // "passive.capacitor.mlcc"
// ir.rs
pub struct SymbolInstance { /* existing */ #[serde(default)] pub role: Option<String> }  // "input_cap", "rf_feed"
```

Bump `SCHEMA_VERSION` with a no-op migration, so older builds reject newer files safely.

`category` stays for display. Rules stop reading it.

### 4.3 Commands (`eak-runtime`)

| Command | Class | Note |
|---|---|---|
| `SetRequirementFacts { requirement_id, facts }` | ReversibleLocal | agent-proposed facts arrive with `confirmed: false` |
| `SetDomainProfile { profile: Option<DomainProfile> }` | EngineeringImpacting | agent needs approval; stores only, no pack lookup, so `eak-runtime` does not depend on packs for this |
| `SetSymbolRole { symbol_id, role: Option<String> }` | EngineeringImpacting | role changes which rules apply |

Classification and composition are **read-only queries**, not commands.

### 4.4 Pack schema (`crates/eak-knowledge/packs/<pack-id>.json`)

```json
{
  "schema": 1,
  "pack": { "id": "eak.power.dcdc", "version": "0.1.0", "title": "Non-isolated DC-DC converters",
            "requires_core_schema": 3, "declared_maturity": "DRAFT" },
  "sources": [ { "id": "SLVA477B", "kind": "app_note", "title": "...", "url": "...",
                 "retrieved": "2026-11-13", "verified_by": null, "verified_on": null } ],
  "families": [ { "id": "ic.regulator.buck", "parent": "ic.regulator",
                  "required_ratings": ["input_voltage_max", "switch_current_limit"] } ],
  "archetypes": [ {
      "id": "buck_sync", "title": "Synchronous buck",
      "identify": { "hard": [ { "signal": "family_present", "is": "ic.regulator.buck", "score": 6 },
                              { "signal": "fact", "key": "topology", "is": "buck", "score": 6 } ],
                    "weak": [ { "signal": "keyword", "any": ["buck", "step-down"], "score": 1 } ],
                    "contradicts": [ { "signal": "fact", "key": "isolation.required", "is": true } ] },
      "blocks": [ { "id": "power_stage",
                    "roles": [ { "role": "switch_controller", "min": 1 }, { "role": "power_inductor", "min": 1 },
                               { "role": "input_cap", "min": 1 }, { "role": "output_cap", "min": 1 } ] } ],
      "rules": ["PWR-DCDC-001", "PWR-DCDC-002"]
  } ],
  "overlays": [ { "id": "switching_noise", "forced_by": [ { "signal": "role_present", "is": "switch_node" } ],
                  "rules": ["PWR-EMI-001"] } ],
  "rules": [ {
      "id": "PWR-DCDC-001", "title": "Inductor saturation margin",
      "stage": "erc", "severity": "error", "safety_critical": false,
      "applies_to": { "role": "power_inductor" },
      "constraint": { "param": "rating.saturation_current.margin", "bound": "min", "value": 1.2, "unit": "ratio" },
      "evaluator": "rating_margin",
      "needs": null,
      "source": "SLVA477B",
      "declared_maturity": "OFFLINE_TESTED", "tests": ["domain::tests::isat_margin"]
  } ],
  "strategies": { "placement": [ { "group": "power_stage", "intent": "minimise_loop_area", "priority": 10 } ],
                  "routing":   [ { "net_role": "switch_node", "intent": "short_and_wide", "priority": 10 } ],
                  "copper":    [ { "net_role": "power_ground", "intent": "solid_pour" } ] },
  "evidence_required": [ { "id": "EV-LOOP", "kind": "review", "blocks_release": true } ]
}
```

Field notes:

- `stage` is one of `requirements, schematic, erc, placement, routing, copper, thermal, mechanical, drc, manufacturing, evidence`. These are the matrix columns.
- `constraint.bound` is explicit: `min`, `max`, `exact`, `require` or `forbid`. DESK infers direction from the parameter name (`min_`, `max_` prefixes); that is fragile, so EAK states it.
- `needs` is `null`, `external_simulation`, `measurement` or `specialist_review`. It lets a pack state a requirement EAK cannot check.
- `safety_critical: true` marks rules that can never be relaxed or dropped.
- `overlays[].forced_by` are hard signals. A forced overlay cannot be left out of a profile.

### 4.5 Classification (`eak_knowledge::classify`)

```rust
pub fn classify(p: &Project, reg: &Registry) -> Classification;

pub struct Classification {
    pub state: ProfileState,                 // Declared | ReviewRequired | Unclassified
    pub archetypes: Vec<Match>,              // every archetype over threshold, not one winner
    pub forced_overlays: Vec<Match>,
    pub suggested_overlays: Vec<Match>,
    pub uncovered: Vec<Signal>,              // observed, but no loaded pack consumes it
    pub missing_facts: Vec<String>,          // what to ask the user
    pub contradictions: Vec<String>,
}
pub struct Match { pub target: PackRef, pub score: u32, pub evidence: Vec<Signal> }
```

Rules of the classifier:

1. **Signals come from the IR, not from prose**: confirmed requirement facts, component families and symbol roles present, declared net voltages and currents, net-class attributes.
2. **Keywords are weak.** They are capped, and keyword score alone can never reach `Declared`; the best it gives is `ReviewRequired`. DESK lesson: 44 of its 100 archetypes score on keywords only.
3. **Mixed domain is the normal case.** Every archetype over threshold is returned. An ESP32 + LoRa + buck board returns three archetypes and their overlays.
4. **Unknown stays unknown.** Below threshold the state is `Unclassified`, no archetype is guessed, and `missing_facts` says what to ask.
5. **`uncovered` is the honesty mechanism.** A part with family `rf.transceiver.lora` and no RF pack at `IMPLEMENTED` or better becomes an uncovered signal, which becomes a finding (4.7).
6. The function only proposes. The stored profile changes only through `SetDomainProfile`.

### 4.6 Conservative composition (`eak_knowledge::compose`)

```rust
pub fn compose(reg: &Registry, profile: &DomainProfile, p: &Project) -> ComposedRuleSet;

pub struct ComposedRuleSet {
    pub rules: Vec<ComposedRule>,            // each with contributions: Vec<(PackRef, rule_id, value)>
    pub conflicts: Vec<Conflict>,            // unresolved
    pub resolutions_applied: Vec<RuleResolution>,
    pub strategies: Strategies,
    pub pack_hashes: BTreeMap<String, String>, // SHA-256 of each pack file used
}
```

| Case | Result |
|---|---|
| Several `min` bounds on one parameter and target | the largest wins |
| Several `max` bounds | the smallest wins |
| `min` above `max` after combining | **Conflict** |
| `exact` values differ | **Conflict** |
| `require` and `forbid` on one target | **Conflict** |
| A conflict | never auto-resolved. It is reported in every check and blocks release until a `RuleResolution` is recorded through `SetDomainProfile`, with actor, rationale and revision. |
| A resolution that relaxes a `safety_critical` rule | rejected. Only a stricter value is accepted. |
| A forced overlay missing from the stored profile | re-derived on every run and reported as an error. Safety constraints cannot disappear by editing the profile. |
| Profile pinned to a pack version this build lacks | error finding; no silent upgrade |

Every composed rule keeps its contributions, so a report can show which pack set which value.

### 4.7 Evaluators in `eak-validation`

New file `crates/eak-validation/src/domain.rs`, producing a fourth check named `DOMAIN`:

```rust
pub struct EvalCtx<'a> { pub project: &'a Project, pub netlist: &'a Netlist,
                         pub copper: &'a [CopperItem], pub rule: &'a ComposedRule }
pub enum EvalOutcome { Findings(Vec<Finding>), NotApplicable(String), Unsupported(String) }
pub type Evaluator = fn(&EvalCtx) -> EvalOutcome;

pub fn evaluators() -> &'static [(&'static str, Evaluator)];   // static table, no dynamic loading
pub fn run(p: &Project, reg: &Registry) -> Report;             // check = "DOMAIN"
```

`Report` gains one optional field (report type change, covered by ADR-0009):

```rust
#[serde(default)] pub coverage: Option<DomainCoverage>,

pub struct DomainCoverage {
    pub state: ProfileState,
    pub profile: Option<DomainProfile>,
    pub rules: Vec<RuleStatus>,          // one per composed rule
    pub conflicts: Vec<Conflict>,
    pub uncovered: Vec<Signal>,
    pub pack_hashes: BTreeMap<String, String>,
}
pub enum RuleState { Evaluated, NotApplicable, NotImplemented, Unsupported,
                     RequiresExternalSimulation, RequiresMeasurement, RequiresSpecialistReview, Conflict }
```

Findings the `DOMAIN` check always raises:

| Condition | Severity | Message intent |
|---|---|---|
| No profile, or `Unclassified` | Warning | "Generic rules only. This is not adequate validation for this design." |
| Uncovered signal | Warning | "No domain knowledge for X. Engineering review required." |
| Safety-critical uncovered signal, such as a net above the low-voltage limit with no isolation pack | Error | blocks release |
| Rule with no evaluator in this build | Warning (Error if `safety_critical`) | `NOT_IMPLEMENTED` |
| Rule with `needs` set | Info, listed | "Requires external simulation / measurement / specialist review" |
| Unresolved conflict | Error | lists both contributions |

First evaluators, all reusing code that already exists:

| Evaluator | Reuses |
|---|---|
| `rating_margin` | `check_rating` in `fm/eak-power-calc:crates/eak-validation/src/erc.rs:24-98`; replaces the literal margins of ERC-011/014/015/016 |
| `role_present` | netlist and symbol roles |
| `decoupling_present` | ERC-010 logic (`erc.rs:50-73,203-217`) |
| `track_ampacity` | DRC-008 (`drc.rs:59-70`) |
| `voltage_clearance` | DRC-002 voltage path (`drc.rs:168,196-199`) |
| `creepage` | `ipc::creepage_mm` (`ipc.rs:173`), which no rule calls today |
| `keepout_clear`, `controlled_impedance`, `diff_pair_skew`, `via_fence_pitch` | return `Unsupported` until the IR items in 4.10 exist |

`eak-manufacturing`'s release gate then also requires a `DOMAIN` report with a declared profile, no unresolved conflict and no safety-critical rule outside `Evaluated`. Otherwise the package stays `DRAFT`.

### 4.8 Maturity and evidence

```rust
pub enum Maturity { NotStarted, Draft, Implemented, OfflineTested, VerifiedAgainstSource,
                    LiveVerified, NeedsHardwareValidation, Unsupported, Blocked }

pub fn maturity_matrix(reg: &Registry, implemented: &[&str], tests: &[&str]) -> MaturityMatrix;
```

The effective state of a rule is the **lower** of what the pack declares and what the build can show:

| State | Granted only when |
|---|---|
| `DRAFT` | the rule exists in a pack |
| `IMPLEMENTED` | its evaluator name is in `domain::evaluators()` |
| `OFFLINE_TESTED` | each name in `tests` exists; a CI test fails if a named test is missing |
| `VERIFIED_AGAINST_SOURCE` | its source has `verified_by` and `verified_on` filled by a human |
| `LIVE_VERIFIED`, `NEEDS_HARDWARE_VALIDATION` | an evidence record with a file hash is attached |
| `UNSUPPORTED` | the evaluator returns `Unsupported` or the rule has `needs` |

A cell of the domain matrix is the **lowest** state among the rules of that archetype and stage, and `NOT_STARTED` when there are none. The matrix is generated, never hand-edited.

### 4.9 RF as a first-class pack

RF needs three things that packs alone cannot give. All three are named in 4.10.

1. Calculations in `eak-engineering/src/rf.rs`: microstrip, stripline and coplanar impedance from stackup geometry, differential impedance, width for a target impedance, guided wavelength. Each returns the value, the method name, its validity range and an accuracy note, and carries a `VERIFIED_AGAINST_SOURCE = false` flag until checked against published reference vectors. DESK has no impedance formula to port (section 3), so this is new work.
2. IR support: inner layers, per-layer dielectric (started in `fm/eak-stackup`), keep-outs, net-class impedance and differential-pair fields.
3. Rules in an `eak.rf` pack: 50 Ω feed, antenna keep-out from the manufacturer datasheet, matching-network topology and placement next to the antenna, continuous return path, via fence pitch, isolation from switching and digital blocks.

Rules EAK cannot evaluate are still listed with `needs`: antenna efficiency and pattern (`measurement`), matching values (`measurement`), radiated emissions (`specialist_review`), full-wave effects (`external_simulation`).

### 4.10 Interfaces per team, in dependency order

No team edits another team's files. Each step lists the files it owns.

| Step | Team | Builds | Files |
|---|---|---|---|
| 0 | eak-core | ADR-0009; empty `eak-knowledge` crate added to the workspace | `docs/decisions/0009-*.md`, root `Cargo.toml`, `crates/eak-knowledge/Cargo.toml` |
| 1 | eak-core | IR types of 4.2; schema bump and migration; three commands of 4.3 | `crates/eak-core/src/{ir,library,migrate}.rs`, `crates/eak-runtime/src/{command,delta}.rs` |
| 2 | eak-electronics | `schema.rs`; `Registry::builtin()` and `Registry::load_dir()` with pack validation; `classify`; `compose`; `maturity_matrix`; first packs `eak.core.generic`, `eak.embedded.mcu`, `eak.power.dcdc` | everything under `crates/eak-knowledge/src` and `/packs` |
| 3 | eak-electronics | `domain.rs` with the evaluator table and `domain::run`; `coverage` on `Report`; move literal margins into `eak.core.generic`; release-gate change | `crates/eak-validation/src/{domain,report,erc,drc,lib}.rs`, `crates/eak-manufacturing/src/lib.rs` |
| 4a | eak-intelligence | read-only tools `classify_project`, `get_domain_coverage`, `get_maturity_matrix`; `describe()` arms for the three commands; prompt rule "state missing domain knowledge; never present generic rules as adequate"; rename `tools::Domain` to `CommandArea` | `crates/eak-intelligence/**` |
| 4b | eak-desktop | IPC `classify_project`, `domain_coverage`, `maturity_matrix`; `DOMAIN` in `run_check`; a "Domain and coverage" panel; wire types | `apps/desktop/**` |
| 5 | eak-core | IR for technology: `CopperLayer::Inner(u8)`, `Via.span`, `Keepout`, slots, `NetClass.impedance_ohm`, `NetClass.diff_pair` | `crates/eak-core/src/ir.rs`, `crates/eak-runtime/src/command.rs` |
| 6 | eak-electronics | `rf.rs`; `switching_converter(Topology, ..)` replacing `buck_*`; `PlaceOptions` and `RouterOptions` built from `Strategies`; multi-layer routing; `eak.rf` pack | `crates/eak-engineering/**`, `crates/eak-knowledge/packs/eak.rf.json` |

Steps 4a and 4b can start as soon as step 1 fixes the type names, working against the signatures in 4.5 to 4.8. Steps 2 and 3 are the critical path. Steps 5 and 6 are needed before any RF, high-speed or mains cell can rise above `DRAFT`.

### 4.11 What to do with the open branches

| Branch | Recommendation |
|---|---|
| `fm/eak-lib-expand` | Keep the generic chip, diode, transistor, crystal and connector builders. Move `board_parts()` out of `eak-core` into library data, and settle one definition per part with the two validation projects. |
| `fm/eak-power-calc` | Reshape to `switching_converter(Topology::Buck, ..)` before merge, as the buck project's own gap G1 asks. Keep `check_rating`: it is already the generic `rating_margin` evaluator. Move the buck wording out of the generic rules. |
| `fm/eak-stackup` | Merge direction is right. Follow with `CopperLayer::Inner`. |
| `fm/eak-val-esp32`, `fm/eak-val-power` | Keep as acceptance evidence. Their gap lists are the best requirements input the teams have. |

---

## 5. Domain compatibility and engineering maturity matrix (first pass)

State of `main` at `bedd9b5`. Open branches are noted in the last column and earn nothing until merged.

How to read it:

- A cell rises above `NS` only for capability that is specific to that domain, or that an automated test on `main` exercises for that domain.
- The generic engine is tested on one board: a 10-part ATtiny85 + LDO design. That is an embedded/MCU board, so it counts as evidence in row 1 only. Those cells are marked `*`: generic engine, no domain rules.
- **Fits core today?** says whether the domain fits inside what the core can represent at all: two layers, low frequency, low voltage, no keep-outs.
- Codes: `NS` not started, `DR` draft, `IM` implemented, `OT` offline tested, `UN` unsupported and declared so in code. No cell is `VERIFIED_AGAINST_SOURCE`, `LIVE_VERIFIED`, `NEEDS_HARDWARE_VALIDATION` or `BLOCKED`.
- Columns: REQ requirements classification, SCH schematic, LIB component library, CALC electrical calculations, PLC placement, RTE routing, CU copper and grounding, TH thermal and mechanical, CHK ERC/DRC, MFG manufacturing, TEST automated test evidence, IV independent verification.

| # | Domain | Fits core today? | REQ | SCH | LIB | CALC | PLC | RTE | CU | TH | CHK | MFG | TEST | IV | Known unsupported; open-branch delta |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Embedded / MCU boards | yes | NS | OT* | DR | OT* | OT* | OT* | OT* | IM* | OT* | OT* | OT* | NS | hierarchy, buses, clock and programming-interface rules, more than 2 layers. One MCU in the library (ATtiny85, unverified ratings) |
| 2 | ESP32, STM32, RP2040, other MCUs | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | module antenna keep-out, USB pair. Branches: ESP32-C3 part in `fm/eak-lib-expand`; ESP32-C3 board in `fm/eak-val-esp32` (draft; hand-routed USB; package not fabricable as-is). STM32, RP2040: nothing anywhere |
| 3 | IoT, wireless sensor nodes, battery-powered | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | power budget, sleep current, battery life, antenna |
| 4 | RF and wireless | no | NS | NS | NS | NS | NS | NS | NS | NS | UN | NS | NS | NS | impedance and signal integrity declared not implemented (`drc.rs:465`); no keep-outs |
| 5 | LoRa, Wi-Fi, Bluetooth, Sub-GHz, GNSS, UWB, cellular | no | NS | NS | NS | NS | NS | NS | NS | NS | UN | NS | NS | NS | band data, module rules, antenna requirements |
| 6 | RF front ends, antennas, matching, transmission lines | no | NS | NS | NS | NS | NS | NS | NS | NS | UN | NS | NS | NS | no impedance calculation in EAK or in DESK; no via fencing; 2 layers |
| 7 | DC-DC buck, boost, buck-boost | partly | NS | NS | NS | NS | NS | NS | NS | DR | IM | NS | NS | NS | zone current density, thermal spreading, loop-area rules. Branches: buck-only maths with unit tests in `fm/eak-power-calc`; buck board in `fm/eak-val-power`, placed and routed by hand. Boost, buck-boost: nothing |
| 8 | AC-DC and isolated supplies | no | NS | NS | NS | OT | NS | NS | NS | NS | IM | NS | NS | NS | CALC is the creepage and clearance functions only, tables unverified. No creepage check, slots, isolation barrier or transformer model |
| 9 | Flyback, forward, push-pull, half- and full-bridge | no | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | everything |
| 10 | Dual-active-bridge, advanced conversion | no | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | everything |
| 11 | BMS, chargers, protection | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | chemistry rules, cell balancing, protection thresholds |
| 12 | Motor control, BLDC, PMSM, inverters, gate drivers | partly | NS | NS | NS | NS | NS | NS | NS | NS | IM | NS | NS | NS | CHK is generic track current and voltage clearance only. No gate-loop, dead-time, bus-capacitor or thermal rules |
| 13 | Analog and precision measurement | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | guarding, grounding partition, noise and tolerance analysis |
| 14 | Sensors, instrumentation, signal conditioning | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | one sensor part exists only in `fm/eak-lib-expand` |
| 15 | High-speed digital, FPGA, processor carriers | no | NS | NS | NS | NS | NS | NS | NS | NS | UN | NS | NS | NS | 2 layers, no hierarchy, no differential pairs, no length matching |
| 16 | USB, Ethernet, CAN, RS-485, industrial interfaces | partly | NS | NS | NS | NS | NS | NS | NS | NS | UN | NS | NS | NS | differential impedance, termination, protection rules |
| 17 | Audio | partly | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | everything domain-specific |
| 18 | High-voltage, isolation, mains | no | NS | NS | NS | OT | NS | NS | NS | NS | IM | NS | NS | NS | CALC as row 8. Creepage refuses pollution degree 3 and above and anything over 1000 V. No insulation coordination |
| 19 | Mixed-signal and multi-domain | no | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | no rule composition exists |
| 20 | New custom domains via packs | no | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | NS | no pack mechanism exists |

Notes on the non-`NS` cells:

- `OT*` in row 1 rests on one end-to-end test, `reference_design_end_to_end`, plus an injected short and clearance test. `eak-validation` has no unit tests of its own.
- `IM` under CHK means DRC-002 (voltage clearance) and DRC-008 (track current) exist but have no dedicated test on `main`.
- `OT` under CALC in rows 8 and 18 means `clearance_mm` and `creepage_mm` pass their tests against DESK's transcribed values (`ipc.rs` tests `clearance_table_rows`, `creepage_rows`). The tables are not verified against the standards and no check uses the creepage function.
- IV is `NS` everywhere: "Independent engineering review and fabricator DFM: not performed" (`STATUS.md`).

### Three separate measures of progress

| Measure | State |
|---|---|
| **Core product completion** | Strong: typed command engine, revision safety, undo, crash recovery, evidence-bound reports, native 2-layer place/route/pour, Gerber and drill. Missing: requirements interpretation, families and roles, rules as data, multi-layer, keep-outs, impedance, hierarchy, thermal model, fab profiles, 3D viewer. Desktop app and agent exist only in open branches. |
| **Domain coverage and maturity** | 0 of 20 domains have an archetype, a pack or a domain-specific rule. 1 of 20 is covered by generic-engine test evidence, and 4 more (rows 7, 8, 12, 18) have a generic calculation or check that applies to them. 10 of 20 do not fit today: 8 need technology the IR cannot represent, and 2 need the pack mechanism itself. |
| **Acceptance projects** | Both are in open, unmerged branches. ESP32 dashboard: draft; its own readiness file marks fabricability "FAIL / NOT VERIFIED" and RF, USB impedance and antenna keep-out NOT VERIFIED. Buck board: built with no auto-placement or auto-routing; switching quantities partly hand-calculated. No hardware exists for either. |

These must stay separate. Both boards passing would move only the third row.

---

## 6. Gap analysis

### 6.1 Against the owner's directive

| Directive requirement | State | Evidence or gap |
|---|---|---|
| Domain-independent core: canonical IR, project and revision management | met | `ir.rs`; `engine.rs:324` `execute`; revision check `command.rs:268` |
| Core: requirements interpretation | absent | S1; `ir.rs:41-45` |
| Core: component, symbol, footprint management | partial | definitions by command; no family, role or editor |
| Core: schematic connectivity | met, flat only | `netlist.rs`; no hierarchy or buses |
| Core: electrical calculations | thin | six functions plus unverified IPC tables |
| Core: placement and geometric reasoning | generic only | A8; no groups or keep-outs |
| Core: routing and copper | generic, 2 layers | A1, A6, A7 |
| Core: 2D and 3D design | not on `main` | 2D viewer in `fm/eak-desktop-shell`; 3D is scene data only (`scene3d.rs`, 93 lines) |
| Core: ERC/DRC and independent verification | generic | 29 rules; coverage not design-aware (A4, A5) |
| Core: manufacturing compilation | met, 2 layers | fab profile unused (S12) |
| Core: evidence, provenance, validation | met for reports | revision and SHA-256 bound (`report.rs:46-61`); no maturity model (S14) |
| Structured, versioned domain knowledge packs | absent | S3; design in section 4 |
| Several archetypes and overlays on one project | absent | — |
| Conservative composition, explicit conflicts | absent | — |
| Safety-critical constraints never silently disappear | not enforceable | the IR has no notion of a safety constraint |
| Unfamiliar project not misclassified | trivially true | nothing is classified |
| Generic rules not presented as adequate validation | **not met** | a clean report reads `NO_ERRORS_IN_IMPLEMENTED_CHECKS` with a design-independent caveat list (A4, A5) |
| Missing domain knowledge and required review stated | **not met** | no mechanism |
| Minimum coverage of 20 domains in the architecture | not met | 10 of 20 do not fit today (section 5) |
| RF first-class: impedance, microstrip and stripline, stackup-aware estimate | absent | S7; εr arrives only in `fm/eak-stackup` |
| RF: 50 Ω feeds, differential pairs | absent | `NetClass` has no such field (`ir.rs:228-237`) |
| RF: antenna placement and keep-outs | absent | S8 |
| RF: matching networks, return paths, via fencing, isolation from switching | absent | — |
| RF: unsupported checks marked as needing simulation, measurement or review | partial | "Controlled impedance and signal integrity" is listed as not implemented (`drc.rs:465`), but for every design, and with no route to simulation or review |
| Maturity matrix with the nine states | absent | `STATUS.md` and the traceability table use PASS/PARTIAL/NOT STARTED/UNKNOWN |
| No maturity credit for a JSON or prompt existing | not applicable yet | enforced by design in 4.8 |
| Progress measured three ways | absent | — |

### 6.2 Against the three source-of-truth documents

The documents do not mention domain packs. The owner's directive is a new product-owner instruction, which `AGENTS.md` says overrides them, so the traceability table and an ADR must record it.

| Source requirement | State | Evidence or gap |
|---|---|---|
| Spec §2.1: one command path for human and agent | met | `engine.rs:324`; `ActorKind { Human, Agent, Cli }` (`engine.rs:18-22`) |
| Spec §7.1: technology baseline | partial | section 7 |
| Spec §7.3: command response shape | met | `CommandResponse` (`engine.rs:68-85`) has operation id, revisions, affected ids, approval, undo reference, validation note |
| Spec §9: AG-03 stale revision, PROC-02 uncertain outcome, MFG-01 traceability | met | `tests/engine.rs`; traceability rows CMD-01, UNDO-01, MFG-01 |
| Spec §9: UI-01, UI-02, 3D-01 | not on `main` | traceability UI-01 "NOT STARTED" |
| Spec §8.1: DESK reuse "isolated behind a temporary compatibility/knowledge layer" | not built | this is the place the `eak-knowledge` crate fills |
| Instructions §7.1 IR: buses and hierarchy | missing | S13 |
| Instructions §7.1 IR: cut-outs, keep-outs, layers | missing | S8, S9 |
| Instructions §7.1 IR: "voltage domains, current/power limits, … assumptions" | partial | per-net voltage and current only (`ir.rs:249-254`) |
| Instructions §7.1 IR: enclosure context | missing | only a maximum height (`ir.rs:257-259`) |
| Instructions §9 Requirements stage: "units, tolerances, assumptions and unresolved items" | missing | S1 |
| Instructions §9 Circuit architecture: "Partition power, control, sensing, protection, RF/high-speed and interfaces … choose topologies" | missing | this is the archetype and block function of section 4 |
| Instructions §9 PCB constraints: "impedance, differential pairs and keepouts" | missing | S7, S8 |
| Instructions §9 Placement: "sensitive analog/RF … with thermal/return-path intent" | missing | `placer.rs` is connectivity only |
| Instructions §9 Routing: "critical nets, power, return paths, differential pairs, clocks … stitch" | missing | `router.rs` has no net priority |
| Instructions §9.1: "Unknown is not pass. Unsupported analysis must be labelled" | met in form, not in substance | labels exist but are static (A4) |
| Instructions §9.1: AI numbers recomputed in Rust | met | `fm/eak-intelligence-core` exposes only `calc_*` tools backed by `eak-engineering` |
| Instructions §5.1 and §6.2: prohibited architecture, Python-free | met | section 7 |
| Reference guide §3.1: classify each DESK finding and keep provenance | met for IPC and ERC | `docs/REPOSITORY_RESEARCH.md:11,28-38`; `ipc.rs:3-12`. DESK's domain layer was not audited there |
| Reference guide closing criterion: "a user describes an electronics design intent; EAK produces a native design" | not met | no path from intent to design exists; requirements are stored and never read |

---

## 7. Technology-stack compliance

### 7.1 Compliance matrix

| # | Frozen layer | State | Evidence |
|---|---|---|---|
| 1 | Rust core engine | **compliant** | five crates on `main` (`Cargo.toml` members); 35 tests pass |
| 2 | Versioned EAK-IR in Rust | **compliant** | `SCHEMA_VERSION = 1` (`ir.rs:12`), `migrate.rs`; version 2 in `fm/eak-stackup` |
| 3 | Tauri 2 | **not on `main`** | only in `fm/eak-desktop-shell`: `tauri = "2"` (`apps/desktop/src-tauri/Cargo.toml`), config schema `https://schema.tauri.app/config/2` |
| 4 | React + TypeScript | **not on `main`** | same branch: `react ^19.3.1`, `typescript ^5.9.0` (`apps/desktop/package.json`) |
| 5 | Vite | **not on `main`** | same branch: `vite ^8.3.8` |
| 6 | SQLite for metadata, index, cache only | **compliant** | `rusqlite 0.37` bundled (`crates/eak-runtime/Cargo.toml`); two tables, recent projects and an event log (`index.rs:35-41`); canonical data is `project.eak.json` |
| 7 | Three.js / React Three Fiber on demand | **absent everywhere** | no `three` or fiber dependency on any branch; Rust produces scene data only (`scene3d.rs`) |
| 8 | One shared typed Rust command system | **compliant** | `Engine::execute` (`engine.rs:324`); the desktop IPC `execute_command`, the CLI `apply`, the agent tools and both validation projects all call it |
| 9 | Native Rust algorithms | **compliant** | router, placer, copper, netlist, ERC/DRC, Gerber and Excellon are first-party Rust; no external EDA call |
| 10 | Windows, Linux, macOS | **not yet evidenced** | CI on `main` head `bedd9b5`: Windows passes, Ubuntu and macOS (arm64) fail, macOS 13 queued. `fm/eak-cli-ci` passes on all four |

| Restriction | State | Evidence |
|---|---|---|
| No Python | **compliant** | 0 Python or Python-packaging files on `main` and all 12 branches (`git ls-tree` scan); `scripts/check-no-python.sh` runs in CI |
| No Node backend services replacing Rust | **compliant** | the only Node packages are frontend and build tools in `fm/eak-desktop-shell`; no server framework |
| No microservices | **compliant** | one process; `docs/ARCHITECTURE.md` "modular monolith" |
| EasyEDA, KiCad, Freerouting not mandatory | **compliant** | no reference in code on any branch except the comment "no Freerouting dependency" (`router.rs:1`) |

### 7.2 Deviations and risks

| # | Deviation | Evidence | Severity |
|---|---|---|---|
| D1 | Desktop layers 3 to 5 exist only in an unmerged branch whose CI was still running | `fm/eak-desktop-shell` | H |
| D2 | No 3D presentation layer on any branch | no `three` in `package.json` | M |
| D3 | Linux and macOS CI fail on `main`. The scripts are committed non-executable (mode 100644), and the two branches that fix this pass, so that is the probable cause. I did not read the failing logs. | `git ls-files -s scripts`; `fm/eak-scripts-exec`, `fm/eak-cli-ci` | H |
| D4 | The desktop's TypeScript wire types are a hand-written copy of the Rust IR types, so the two can drift | `fm/eak-desktop-shell:apps/desktop/src/types.ts` | M |
| D5 | Declared minimum Rust is 1.80, but the code uses `Option::is_none_or`, which to my knowledge is stable only from 1.82. Not tested on 1.80. | `Cargo.toml:18`; `placer.rs:128`, `copper.rs:515`, `drill.rs:92` | L |
| D6 | No licence gate in CI, although `LICENSING.md` requires a full inventory before any installer | `.github/workflows/ci.yml` | M |
| D7 | awk and shell inspection tools in a validation project. Not Python, and outside the product build. | `fm/eak-val-power:validation/power-buck/tools/` | L |
| D8 | The Python-free check looks only at tracked file names | `scripts/check-no-python.sh:5` | L |

Not deviations, noted for clarity: `fm/eak-val-esp32` contains a small Rust HTTP server for the acceptance dashboard and ESP32 firmware. Both are validation artefacts in their own workspaces, not EAK services.

### 7.3 Dependency and licence table

`main`, resolved with `cargo tree --workspace --offline --format "{p} | {l}"` (57 lock entries including the five EAK crates):

| Dependency | Version | Licence | Use |
|---|---|---|---|
| serde, serde_json | 1.0.229, 1.0.154 | MIT OR Apache-2.0 | serialisation |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | errors |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | evidence hashes |
| uuid | 1.29.2 | Apache-2.0 OR MIT | object ids |
| rusqlite, libsqlite3-sys | 0.37.0, 0.35.0 | MIT; bundled SQLite is public domain | metadata index |
| foldhash (transitive) | 0.1.5 | Zlib | — |
| memchr (transitive) | 2.8.3 | Unlicense OR MIT | — |
| unicode-ident (transitive) | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | — |
| generic-array, zmij (transitive) | 0.14.7, 1.0.23 | MIT | — |
| all other transitive crates | — | MIT OR Apache-2.0 | — |

No GPL, LGPL, AGPL or MPL crate on `main`. The workspace itself is proprietary (`license = "UNLICENSED"`).

Added by open branches. I read the manifests but did **not** resolve these licences:

| Branch | Adds | Lock entries | To check before merge |
|---|---|---|---|
| `fm/eak-intelligence-core` | `ureq 3` with rustls; brings `ring`, `rustls`, `rustls-webpki`, `webpki-roots` | 88 | `ring 0.17.14` is "Apache-2.0 AND ISC" and `webpki-roots 1.0.12` is "CDLA-Permissive-2.0" (read from the local cargo registry); both need a legal read |
| `fm/eak-desktop-shell` | `tauri 2`, `tauri-build 2`; npm: `@tauri-apps/api`, `react`, `react-dom`; dev: `@tauri-apps/cli`, `vite`, `vitest`, `typescript`, `@vitejs/plugin-react`, `jsdom`, `@testing-library/react` | 433 | full transitive inventory; on Linux the app links system WebKitGTK and GTK (LGPL) |
| `fm/eak-cli-ci` | no new third-party crate | 58 | — |
| `fm/eak-val-esp32`, `fm/eak-val-power` | their own workspaces and lock files, including ESP32 firmware crates | separate | outside the product build; not inventoried |

---

## 8. Recommendations

1. **Start the pack work now, in the order of 4.10.** Steps 0 and 1 (ADR, IR fields, three commands) unblock all four teams. Steps 2 and 3 are the critical path.
2. **Fix the honesty gap first inside step 3.** Until a `DOMAIN` check exists, a clean ERC/DRC on an RF or mains design reads as reassurance it has not earned (A4, A5). This is the one finding that can mislead a user today.
3. **Reshape two branches before they merge**: `fm/eak-power-calc` to a topology-parameterised API, and `fm/eak-lib-expand` to move the board parts out of `eak-core` (4.11).
4. **Make the IR able to represent the missing technologies** (step 5): inner layers, via spans, keep-outs, slots, net-class impedance and differential pairs. Eight of twenty domains are blocked on this, not on knowledge.
5. **Treat the two validation projects' gap lists as the backlog.** They are accurate and already name an owner per gap.
6. **Get Linux and macOS green on `main`** by merging the script-permission fix, and add a licence gate to CI before the desktop branch lands.
7. **Record the directive** in `docs/REQUIREMENTS_TRACEABILITY.md` and replace the prose in `STATUS.md` with the generated matrix once 4.8 exists.

Nothing here needs a decision from the owner: the directive already sets the direction, and every item above is a recommendation for firstmate to route.

## 9. Commands used

```
git fetch origin --prune; git ls-remote origin
gh-axi pr list; gh-axi pr view <n>; gh api repos/.../pulls, .../commits/<sha>/check-runs, .../actions/runs
git diff --stat origin/main...origin/<branch>; git grep -n <pattern> origin/<branch> -- <path>; git show origin/<branch>:<path>
git ls-tree -r --name-only origin/<branch> | grep -E '\.py$|requirements…'   (all 13 refs: 0 hits)
CARGO_BUILD_JOBS=2 cargo test --workspace --release                          (main: 35 passed, 0 failed)
cargo tree --workspace --offline --format "{p} | {l}"
git clone --depth 1 https://github.com/NijoP/DESK  (read only, at 5ad8492)
pdftotext -layout docs/source-of-truth/*.pdf
```
