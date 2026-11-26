# DESK → EAK domain and workflow mapping

*Scout report for eak-desk-study (goal M1.3, execution brief §7). The body below is the full content
intended for `docs/research/DESK_TO_EAK_DOMAIN_WORKFLOW.md`; a ship task commits it.*

- **DESK:** `main` at `9e44e7ca7d64b4997b5cf1c8afcd7f355b34c79b` (2026-11-21, "Merge develop into main:
  mutation kernel, performance baseline (#18)"), shallow-cloned read-only.
- **EAK:** worktree at `500e938` (detached `main`).
- **Tags:** **[OBSERVED]** = read in source or data; **[INFERRED]** = my conclusion, not stated in source.
- **Paths:** DESK paths are relative to the DESK root; EAK paths are relative to the EAK root.
- **Clean room:** this document describes concepts, schemas and knowledge. It contains no DESK Python
  source and must not be used to port Python into EAK.

## 0. Summary

**The five findings that matter most:**

1. **DESK's discipline is better than its enforcement.** It *documents* strong rules: gate R
   requirements, gate B BOM, a 7-gate block loop, fail-closed rules, read-back after every mutation and a
   single writer. In code, only part of each is enforced:
   - Gate R and gate B are advisory. Gate R counts non-CONFIRMED markdown rows; gate B counts "✅"
     characters.
   - Only 4 live tools use the read-back contract.
   - An approval id is checked only for being non-empty.
   - A BLOCKING rule with missing inputs does not block.
   - Text QA entries are self-certified `pass: true`.

   EAK should port the *rules* and enforce them in `Engine::execute`, `eak-validation` and
   `eak-engineering`.
2. **None of DESK's engineering data is verified against a standard:**
   - `desk/ipc.py:39` `verified_against_standard = False`;
   - the IEC 60664-1 dataset is entirely PROVISIONAL;
   - all 46 references are REFERENCE_ONLY;
   - all 100 archetypes say "automated generation, pending human review".

   This had real consequences. The UPS-1000 live test FAILED (46/100) mainly because an *unverified*
   creepage value was enforced as router clearance on 1.27 mm-pitch ICs, which made the board
   unroutable (`live-test/ups-1000/ROUTING_BLOCKERS.md`). §4.3 lists every flagged item.
3. **Requirements need one store, inside EAK-IR.** DESK has two unconnected representations: the
   markdown `REQUIREMENTS.md` (with status) and `requirements.json` (features, no status). EAK
   currently has three:
   - IR `Requirement{id,text}`;
   - the harness `design-flow/spec.json` side file, written outside `Engine::execute`;
   - a computed, unstored `systems::Requirement`.

   The schema in §2 merges them. It was sent early for M4.1.
4. **EAK has already ported DESK's domain registry and classifier, but not its rule engine.**
   ADR-0011 (`docs/decisions/0011-desk-domain-intelligence.md`) embeds 100 archetypes, 14 domains and
   17 overlays plus the classifier. DESK's 66-rule catalogue, the rule composer, the evidence ladder
   transitions and the profile compiler are not ported (`crates/eak-intelligence/src/domain/classify.rs:16`
   NOT_IMPLEMENTED: rule ids "listed, not executed"). That is the highest-value next slice after
   requirements.
5. **The live tests are the best acceptance material:**
   - ESP32-S3 run 1: FAIL, 50/100.
   - ESP32-S3 run 2b: PARTIAL PASS, 81/100, 66.7 of 225 min, DRC 0, 43/43 nets.
   - UPS-1000: FAIL, 46/100.

   Each has frozen-by-hash requirements, a failure register and an independent examiner. Their failure
   categories (false-success APIs, partial mutations, DRC-parser lies, unroutable spacing, GND opens
   after copper) map directly onto EAK acceptance tests (§11).

**Recommended implementation order (§12):**

1. Requirements record in IR (M4.1).
2. Rule-record schema plus evidence ladder plus composer, ported as data + Rust.
3. A unified result vocabulary with `UNSUPPORTED`/`BLOCKED`/`NOT_EVALUATED`, where a BLOCKING rule that
   is NOT_EVALUATED blocks.
4. Insulation/spacing split (clearance vs creepage vs fab minimum) with per-cell provenance.
5. Component evidence chain: equivalence, the 7-state inventory and the evidence-record validator.
6. Bound approvals, PARTIAL/UNKNOWN outcomes kept pending, and verifier-per-command.
7. Release status ladder.
8. Acceptance fixtures replaying ESP32-S3 and UPS-1000.

## 1. Method and evidence

- **Clone:** `gh repo clone NijoP/DESK <scratch>/DESK -- --depth 1`. Read-only. No DESK, EasyEDA, Python
  or cargo command was run (laptop constraint in the brief).
- **Context Engine:** DESK has one (`docs/07_CONTEXT_ENGINE.md`, `desk/context.py`, `tools/ctx.py`). I
  did not *run* it, because the brief forbids running DESK. I followed its documented relations by hand:
  - block → requirement IDs;
  - ports → nets;
  - MPN → `library/pins`;
  - ledger → spec hash;
  - the gate order at `desk/context.py:295-302`.

  I read focused files instead of whole directories.
- **Reading split:** four parallel read-only readers covered (a) domain intelligence and calculations,
  (b) component, sourcing and release, (c) mutation, workflow and live tests, (d) EAK equivalents. I read
  the requirements path, the evidence policy, the capability matrix and the live-test verdicts myself.
- **Spot-checks:** I re-read the following claims in source and all matched:
  - `desk/ipc.py:39`;
  - `profile_compiler.py:64-73` (BLOCKED only on FAIL);
  - `component_intel.py:249` (`verified=True`);
  - `tools/release.py:47-48` (a skipped DRC counted as ok);
  - `specifications/engine.py:89` ("M" read as 1e-3);
  - `ops.py:212` (approval is a truthiness check);
  - the run-1 FAIL verdict.
- **DESK's own maturity labels contradict each other:**
  - `docs/09_STATUS.md` marks trace width "IMPLEMENTED_AND_TESTED_OFFLINE" and says "Live testing has NOT
    yet been run".
  - `docs/product_assessment/PRODUCT_CAPABILITY_MATRIX.md` (2026-11-11) marks trace width "BROKEN
    integration", unknown-current fail-closed "BROKEN", and clearance/creepage "BROKEN above 50 V".
  - `live-test/` records three live runs on 2026-11-12.

  So **no DESK status label is taken as evidence here**; only code and recorded runs are.
- **EAK's STATUS.md also needs care.** It labels the intelligence, harness and gates rows "code-reviewed
  only, never compiled" (`STATUS.md:102-107,122`). Its "Known gaps" also states that CI ran
  `cargo test --workspace` green on all three OSes at `b62d0e2`. [INFERRED] The per-row labels may be
  stale. Treat EAK maturity as "tests exist", not "tests ran", until CI evidence names the commit.

## 2. Requirement dimension and status schema (priority 1, feeds M4.1)

### 2.1 What DESK actually does
- **10 dimensions** [OBSERVED] `docs/00_REQUIREMENTS.md` §"The 10 requirement dimensions": Function ·
  Electrical-supply · Electrical-budget · Interfaces · Mechanical · Environmental · Regulatory/safety ·
  Manufacturing · Cost & supply · Lifecycle & firmware. Rule: "'unknown' is allowed, blank is not".
- **3 statuses** [OBSERVED] `templates/REQUIREMENTS.md:4`: `CONFIRMED`, `PROVISIONAL` (assumption,
  risk noted) and `TBD` (dependants wait). UPS_1000 sharpens them (`projects/UPS_1000/REQUIREMENTS.md:5`):
  CONFIRMED = client-stated; PROVISIONAL = builder engineering decision, documented and reversible;
  TBD = missing, blocks what depends on it.
- **Row shape** [OBSERVED] `templates/REQUIREMENTS.md` §2: `ID (R-NN) | Dimension | Requirement |
  Value / limit | Source | Status | Risk if wrong`. Units live inside the free-text value cell. There is
  no typed tolerance, owner or timestamp.
- **Gate R also needs derived tables** [OBSERVED] `docs/00_REQUIREMENTS.md` §"Derivations you must
  write down" and `templates/REQUIREMENTS.md` §1, §3–8: operating modes (OFF/SLEEP/ACTIVE/CHARGING/FAULT,
  what is ON, duty); power budget per mode (load × current × duty → average → battery life, with a
  20–30 % margin); voltage tree (rail, source, min/typ/max, loads); worst-case input; mechanical envelope
  (zones, max height top and bottom, keep-out reason); thermal check ((Vin−Vout)·I, θJA → Tj); external
  interface table (pin, net, direction, level, current, ESD exposure); and the GPIO/pin budget
  (strap/boot, level at reset).
- **Gate R exit criteria** [OBSERVED] `docs/00_REQUIREMENTS.md` §"Exit criteria":
  - all 10 dimensions filled (a value, or a PROVISIONAL value plus its risk);
  - the derivations written;
  - operating modes listed, including what is OFF at reset and during a fault;
  - the client confirmed the table (date and name).

  `AGENTS.md` §F restates it as a hard gate before part selection.
- **Enforcement is advisory** [OBSERVED] `desk/context.py:82-92` (`req_rows`) regex-parses markdown rows
  `| R-NN | … |`; the status is the first cell equal to CONFIRMED/PROVISIONAL/TBD.
  `desk/context.py:323-326` (`Engine.brief`) only *reports* "N reqs, open: …". Nothing refuses part
  selection. Test: `tests/test_context.py::test_requirement_rows_indexed`. The enforcing part is block
  G1 (`docs/03_BLOCK_WORKFLOW.md`: "All requirements it needs are CONFIRMED … If not → stop"), with
  block specs listing their `requirements` IDs (`desk/context.py` `block_chunk` refs;
  `tests/test_context.py::test_block_pack_links_requirements_and_neighbours`).
- **A second, unconnected requirement representation** [OBSERVED] `projects/UPS_1000/requirements.json`
  is a flat feature dict (`text`, `voltage_rails{net:V}`, `current_limits_A{net:A}`, `peak_current_a`,
  `continuous_current_a`, `switching_frequency_hz`, `protocols`, `layer_count`, `pollution_degree`,
  `material_group`, `isolation`, `mains`). It has no status and no source. It feeds
  `desk/domain_intelligence/classifier.py:63` `extract_requirements` (regex units → features) and the
  profile compiler. [INFERRED] Nothing reconciles the JSON with the markdown table.
- **Missing-information list** [OBSERVED] `desk/domain_intelligence/classifier.py:38-40` `_CHECKLIST`:
  supply voltage/range, load current or power, power source, interfaces, environment/temperature,
  mechanical outline/enclosure, quantity and assembler. `classifier.py:303-314` adds, by domain:
  - the archetype's first 4 `typical_input_requirements`;
  - for mains/HV: "voltage_rails: every mains/HV net with its working voltage (fail-closed)";
  - for a battery overlay: "battery chemistry".

  Tests: `tests/test_requirement_classifier.py::test_structured_fields_override_text`,
  `::test_a_voltage_beside_a_current_is_never_read_as_a_current`, `::test_ranges_with_one_unit_still_parse`,
  `::test_mains_is_never_silently_dropped`.
- **Real-use pain points** [OBSERVED] `projects/UPS_1000/REQUIREMENTS.md`:
  - **Status is needed per value, not per row.** R-01 is "CONFIRMED (function) / PROVISIONAL (numbers)"
    (line 29); R-04 is "CONFIRMED / RS485 deferred" (line 32).
  - **The source must separate client from builder.** R-02 is "client §5 (24–48 V), directive §4 (24 V),
    builder extension to 58.4 V" (line 30).
  - **Derivation catches inconsistent client data.** M-01: "48 V nominal, 16S LiFePO4" is inconsistent,
    since 16S × 3.2 V = 51.2 V (line 117).
  - **Gate R is scoped.** "PASSED for the low-voltage prototype only … NOT PASSED for the system / HV
    design" (line 132). Open items M-01..M-13 block named downstream work (M-03 "blocks HV release").
  - **Standard editions are requirements too.** M-04: "DESK tables marked UNVERIFIED" (line 120).
  - **WATCH mixes two things in one column.** `projects/WATCH/REQUIREMENTS.md` has 20 rows over the 10
    dimensions (sources like "DOCX §3", "Wearable standard", "Engineering best practice"). Its thermal
    table's "Status" column says PASS for a hand calculation, so the engineering check result and the
    requirement status share a column. EAK must keep them apart.

### 2.2 What EAK has today
- `crates/eak-core/src/ir.rs:116-120` [OBSERVED]: the persisted `Requirement { id, text }`, free text only.
  `Command::AddRequirement { text }` is at `crates/eak-runtime/src/command.rs:19,1307`.
- `crates/eak-intelligence/src/systems/model.rs:21-215` [OBSERVED] has a rich **computed** `Requirement`:
  - `category` (Electrical/Mechanical/Thermal/Environmental/Manufacturing/Safety/Commercial/Unclassified);
  - `quantity`, `value: Value{Unknown|Number|Range{min,nominal,max}|Text}`, `unit`, `tolerance`;
  - `priority` (Must/Should/May);
  - `source: SourceRef{kind: DesignSpec|IrRequirement|Document|Person|Assumption|Derived, reference}`;
  - `evidence`;
  - `knowledge` (Unknown < Assumed < Stated < Confirmed < Verified);
  - `affects`, `constraints`, `validation_method`;
  - `approval` (NotRequired|Pending|Approved{by}|Rejected{by,reason}).

  `Requirement::promote` (model.rs:150-200) already encodes DESK's discipline: an unknown value cannot be
  promoted; an agent may only set `Assumed`; `Confirmed` needs a human plus evidence; a deterministic
  check may set `Verified` only from `Stated` or above.
- That model is **recomputed and never stored** [OBSERVED] (`systems/compile.rs:108` LIMITATIONS). It is
  built from `ir_requirements` (compile.rs:150: IR text → `Knowledge::Stated`) and the harness `DesignSpec`.
- `crates/eak-harness/src/design.rs:176` [OBSERVED] defines the typed `DesignSpec`: rails with
  v_min/v_nom/v_max/i_max_a, conditions, safety{isolation, pollution_degree, overvoltage_category},
  interfaces, protection, acceptance, assumptions. The harness tool `design_spec_set` stores it as
  `design-flow/spec.json` (`design.rs:44-45,1119`), **outside EAK-IR and outside `Engine::execute`**.
  [INFERRED] That conflicts with the AGENTS.md rules "one canonical EAK-IR" and "every design mutation
  goes through Engine::execute", and repeats DESK's markdown-vs-JSON split. M4.1 should fold both into
  one IR record.

### 2.3 Proposed EAK-native requirement schema (for M4.1)
Persist it in EAK-IR (`Project::requirements`, schema bump plus migration) and mutate it only through
typed `Command`s (for example `SetRequirement`, `ConfirmRequirement`, `MarkRequirementNotApplicable`;
`DERIVED` is written only by the engine). This is a shared IR contract, so it needs an ADR.

**RequirementRecord**

| Field | Type | Rule |
|---|---|---|
| `id` | stable `R-NN` | never reused; superseded, never deleted (DESK ledger rule, `desk/ledger.py:1-13`) |
| `dimension` | enum of 10 (below) | required |
| `quantity` | machine key (`supply.vin`, `budget.avg_current`, `env.ambient_max` …) | required when `value` is numeric, so checks can bind |
| `statement` | text | the human words, verbatim |
| `value` | `Unknown \| Number \| Range{min,nominal,max} \| Text \| Enum \| Bool` | `Unknown` is explicit; empty or blank is refused |
| `unit` | `eak_core::units::Unit` | required for Number/Range; unit-checked |
| `limit_kind` | `Max \| Min \| Range \| Exact \| Target` | how a check compares against it |
| `tolerance` | `eak_core::units::Tolerance` | optional |
| `condition` | `{mode?, rail?, temp_c?, scope?}` | the mode, rail or variant/scope (such as "lv_prototype") it holds under |
| `status` | `UNKNOWN \| NEEDS_CONFIRMATION \| CONFIRMED \| DERIVED \| NOT_APPLICABLE` | state machine below |
| `source` | `{kind: ClientDocument\|Person\|Datasheet\|Standard\|Calculation\|BuilderDecision\|AgentProposal, reference, locator (page/§/table), edition?}` | required unless UNKNOWN |
| `risk_if_wrong` | text | required for NEEDS_CONFIRMATION (DESK PROVISIONAL rule) |
| `rationale` | text | required for NOT_APPLICABLE and for a BuilderDecision source |
| `derived_from` | `{calc_id, inputs:[req id@revision], evaluator, evaluator_version}` | required for DERIVED |
| `confirmed_by`, `confirmed_at` | person, timestamp | required for CONFIRMED (DESK: "date + name") |
| `owner` | person/role | required for UNKNOWN and NEEDS_CONFIRMATION (who must answer) |
| `priority` | `Must \| Should \| May` | reuse `systems::Priority` |
| `updated_at`, `revision` | timestamp, IR revision | set by the engine, never by the caller |
| `affects` | block/net/component ids | requirement → block → net traceability (DESK block `requirements` refs) |
| `parts` | `[{key, value, status, source}]` (optional) | one record can hold sub-values with different statuses (UPS_1000 R-01) |

**Dimensions** (DESK's 10, widened with the brief's §8.1 content):

| # | `dimension` | Must capture |
|---|---|---|
| 1 | `function` | function and performance targets, accuracy, update rate, operating modes (OFF/SLEEP/ACTIVE/CHARGING/FAULT, state at reset and during a fault) |
| 2 | `supply` | source types, rail tree, min/nom/max per rail, worst-case transients, reverse polarity, hot-plug |
| 3 | `power_budget` | peak, continuous, average and sleep current per mode; power budget; battery life with margin |
| 4 | `interfaces` | connectors, protocols, external pinouts, cable length, ESD exposure, what others fixed |
| 5 | `mechanical` | outline, dimensional limits, height per zone (top/bottom), mounting, enclosure, connector positions |
| 6 | `environmental` | operating/storage temperature, humidity/condensation, vibration, ingress (IP), UV, altitude |
| 7 | `safety_regulatory` | market/certifications, isolation class per barrier, OVC, pollution degree, material group/CTI, standard editions |
| 8 | `manufacturing_test` | quantity, fabricator/assembler, sides, process, test method, programming method |
| 9 | `cost_supply` | BOM target at quantity, second source, stock risk, lifecycle/EOL |
| 10 | `lifecycle_firmware` | update/OTA, serviceability, variants, expansion, spare GPIO |

**Status semantics**

| Status | Meaning | DESK | EAK `Knowledge` today | Who may set it |
|---|---|---|---|---|
| `UNKNOWN` | the value is not known; `value` must be `Unknown`; dependants are blocked | TBD | Unknown | anyone |
| `NEEDS_CONFIRMATION` | a value exists (client-stated but unconfirmed, a builder decision, or an agent proposal) and is not confirmed; `risk_if_wrong` required | PROVISIONAL | Assumed / Stated | an agent (proposal only) or a human |
| `CONFIRMED` | a responsible human confirmed the value, with evidence | CONFIRMED | Confirmed | **human only**, with evidence (the existing `promote` rule) |
| `DERIVED` | computed by a deterministic `eak-engineering` evaluator from inputs that are all CONFIRMED or DERIVED | §"Derivations" tables | — | **deterministic evaluator only** |
| `NOT_APPLICABLE` | does not apply, with a rationale | "N/A and stated as such" (UPS_1000:108) | — | human only; refused when the classification makes the dimension mandatory (for example `safety_regulatory` under mains/HV) |

- **Keep "the design meets it" out of the requirement status.** Compliance is a separate per-revision
  axis computed by checks: `PASS | FAIL | UNKNOWN | NOT_RUN | UNSUPPORTED | BLOCKED`. The existing
  `Knowledge::Verified` belongs on that axis, not on the record. (DESK mixes the two in the WATCH thermal
  table.)
- **Staleness.** A DERIVED record stores its inputs' revisions. Any input change drops it to
  NEEDS_CONFIRMATION (stale) until the evaluator re-runs. DESK analogue: verification carries the spec
  hash and goes STALE (`docs/07_CONTEXT_ENGINE.md` §ledger;
  `tests/test_context.py::test_gate_progression_and_staleness`).
- **Downgrades are always allowed; upgrades follow the table.** Changing the value of a CONFIRMED record
  drops it to NEEDS_CONFIRMATION.

**Completeness and gating** (DESK gate R + G1, made fail-closed and scoped):
- `requirements_completeness(project, scope)` returns per-dimension counts by status, plus
  `missing_dimensions`, `missing_derivations` and `domain_required_missing`. The last comes from the
  classification: port DESK `_CHECKLIST`, archetype `typical_input_requirements`, mains/HV "working
  voltage per HV net" and battery "chemistry".
- Gate R holds **per scope**, not globally (UPS_1000:132). `lv_prototype` can pass while `system_hv`
  stays blocked.
- `operation_blocked_by(op, scope)` returns the requirement ids and the reason. Starter table
  [INFERRED from `docs/00`, `docs/00b`, `docs/03`]:

| Operation | Needs CONFIRMED or DERIVED in |
|---|---|
| domain classification (draft) | nothing (it runs on partial data; the missing list is its output) |
| architecture/part selection (gate B) | supply, power_budget, mechanical (envelope, height), environmental (temperature range), manufacturing_test (assembler/process) |
| block plan (G1) for block X | every record whose `affects` includes X |
| spacing/creepage rule set | safety_regulatory (isolation per barrier, OVC, pollution degree, material group, standard edition) plus a working voltage per HV net |
| trace/copper sizing | power_budget (current per net) plus copper weight/stack-up |
| release | every Must record in the release scope CONFIRMED, DERIVED or NOT_APPLICABLE |

- An UNKNOWN or NEEDS_CONFIRMATION value that materially changes architecture or safety creates a
  **targeted question** to its `owner` (DESK `docs/00_REQUIREMENTS.md` §"How to interview the client";
  brief §8.1). Questions are generated from `domain_required_missing` and from blocked operations, not
  written free-form.

**Migration**
- IR `Requirement{id,text}` becomes `{statement:text, value:Unknown, status:UNKNOWN,
  dimension:(inferred, or Unclassified), source:{Person,"migrated IR text"}}`.
- Each harness `design-flow/spec.json` rail becomes supply/power_budget records with status
  NEEDS_CONFIRMATION and source `{ClientDocument,"design-flow/spec.json"}`.
- **Nothing is ever auto-CONFIRMED.**

**Tests for the M4.1 worker** (each mirrors DESK evidence):
1. A blank value is refused; `Unknown` is accepted.
2. An agent cannot set CONFIRMED, DERIVED or NOT_APPLICABLE.
3. A DERIVED record goes stale when an input revision changes (analogue of `test_gate_progression_and_staleness`).
4. Gate state is computed, never read from a stored flag (analogue of `tests/test_context.py::test_status_field_cannot_fake_progress`).
5. NOT_APPLICABLE on `safety_regulatory` is refused under a mains/HV classification (analogue of `test_mains_is_never_silently_dropped`).
6. Scoped gate R passes `lv_prototype` and blocks `system_hv` (a UPS_1000 replay).
7. Migration keeps every text verbatim and confirms nothing.


## 3. Domain, archetype and overlay schema; rule composition

### 3.1 DESK model [OBSERVED]
- **Library** `library/pcb_domains/`: `taxonomy.json` (schema 1.0.0, generated 2026-11-12) holds:
  - 14 domains: embedded_mcu, rf_wireless and dcdc_power have 10 archetypes each; ac_mains_hv,
    battery_bms, motor_control, analog_sensors, high_speed_digital and interfaces_comms have 8 each;
    audio_video, industrial_control, led_display and test_debug have 5 each; automotive has 2.
  - 100 archetypes, 17 overlays, 206 component families, 66 rules in 10 rule sets, and 46 references.
  - JSON schemas live in `schemas/`. The allocation is hard-coded in
    `desk/domain_intelligence/validation.py:14-17,49-56`.
- **Archetype** (`schemas/archetype.schema.json`) fields:
  - identity: `archetype_id`, `name`, `domain`, `description`;
  - inputs: `intended_applications`, `typical_input_requirements`;
  - electrical: `electrical_characteristics`, `operating_voltage_ranges` (strings such as "12-60 V"),
    `current_power_characteristics`;
  - parts: `typical_component_families`, `required_component_roles`, `optional_component_roles`;
  - constraints, each a list of prose: `critical_electrical_constraints`, `stackup_recommendations`,
    `placement_rules`, `routing_rules`, `copper_ground_requirements`, `thermal_considerations`,
    `emi_emc_considerations`, `protection_requirements`, `verification_requirements`,
    `manufacturing_constraints`;
  - `applicable_standards` (reference ids), `unsupported_conditions`, `typical_overlays`;
  - `classification{keywords{term:weight}, features}`;
  - `maturity{status, evidence, rule_ids, last_reviewed, reviewed_by}`.
- **Overlay** (`schemas/overlay.schema.json`): `overlay_id`, `applies_when` (all/any conditions over
  features), `adds_component_families`, `placement_rules`, `routing_rules`, `copper_rules`,
  `emi_emc_rules`, `verification_requirements`, `rule_ids`, and `precedence`. Precedence values:
  mains_safety_isolation 100, galvanic_isolation 80, high_current and automotive_environment 70,
  battery_management 65, motor_drive and rf_communication 60, switching_power and emi_emc 55, … ,
  harsh_environment and low_power 30. `mains_safety_isolation` applies when `mains`, when
  `input_voltage_v ≥ 60`, or when `isolation`.
- **Rule** (`desk/domain_intelligence/models.py:76-103`, `schemas/rule.schema.json`) fields:
  - `rule_id` (e.g. `ELEC-CREEP-004`), `version`, `domain`, `category`;
  - `scope{archetypes|"*", overlays, component_families}`;
  - `severity` (BLOCKING > ERROR > WARNING > ADVISORY);
  - `applicability{all,any}`, with operators `== != > >= < <= in intersects between exists`. A missing
    field makes every operator false except `exists` (`rule_engine.py:16-54`);
  - `required_inputs` (names only, no units), `evaluation_method` (evaluator id or `advisory`),
    `parameters`;
  - `source_references`, `verification_method`, `status` (evidence state);
  - `enforcement{subsystem, mode: ENFORCED|ADVISORY|UNSUPPORTED}`, `human_review_required`, `rationale`.

  Units exist only as field-name suffixes (`_v`, `_a`, `_mil`, `_pct`).
- **Outcomes** (`models.py:6-13`): PASS, FAIL, NOT_EVALUATED (inputs missing or not applicable),
  DEFERRED (the owning subsystem applies it at build/route time), ADVISORY, UNSUPPORTED (unknown
  evaluator, `rule_engine.py:350-352`). An evaluator exception produces FAIL (`:353-356`). The 17
  subsystem evaluator ids return DEFERRED (`:328-339`).
- **Classifier** (`classifier.py`):
  - Features are extracted by regex; structured fields override text (`:151-153`).
  - Scoring uses keyword weights plus feature matches and penalties. Voltage in range scores +3, a
    protocol +3 (capped at 6), RF band +4/−3, motor type +4/−3, mains +4/−2, isolation +2/−3. Every
    point is kept as a reason or penalty.
  - CLASSIFIED needs score ≥ 8 and a margin ≥ 3. REVIEW_REQUIRED applies at score ≥ 4 with a small
    margin. Anything lower is UNCLASSIFIED (`:262-269`).
  - An interface-domain winner is demoted when a controller is named (`:249-255`).
  - Mains involvement under a non-mains primary forces REVIEW_REQUIRED and a recorded conflict; it is
    never resolved silently (`:274-294`).
  - Overlays are the primary's `typical_overlays` plus every overlay whose `applies_when` matches.
- **Composition** (`rule_composer.py`):
  1. Scope match: `*`, archetype (primary and secondary treated alike), overlay or family.
  2. Sort by severity, then the highest overlay precedence, then id.
  3. Group by (category, evaluator, families, numeric parameter key). When values differ inside a group,
     the strictness direction is taken from the key pattern:
     - `max_*`, `*tolerance`, `drop_budget`, pitches: smaller is stricter;
     - `min_*`, `margin`, `derating`: larger is stricter;
     - unknown direction: `review_required`, and **both rules stay**.

     The stricter rule wins (`stricter_kept:<id>`). A loser is dropped only if it is no more severe
     (`:63-74`). **A more severe rule is never dropped.**
- **Compiled profile** (`profile_compiler.py`, `models.py:201-218`): primary/secondary archetypes,
  overlays, families, rule ids split into enforced/advisory/unsupported, evaluations, conflicts,
  blocking issues, missing information, `design_profile`, a `status` of READY/REVIEW_REQUIRED/BLOCKED,
  and `library_version = schema@generated`. The compiler copies declared rails and currents only and
  never invents them (`:25-35`; `tests/test_profile_compiler.py::test_never_invents_voltages_or_currents`).
  `unknown_current_policy` is always `fail`.

### 3.2 DESK defects to fix when porting
- **NOT_EVALUATED on a BLOCKING rule does not block** [OBSERVED]. BLOCKED is set only on FAIL
  (`profile_compiler.py:64-73`). UPS_1000 compiled with `DCDC-ISAT-120` and `DCDC-VIN-121` (both
  BLOCKING) NOT_EVALUATED and was not blocked (`projects/UPS_1000/domain_validate.json`).
- **Rule PASS often means "a value could be derived", not "the board meets it"** [OBSERVED].
  `trace_width`, `clearance`, `creepage`, `via_current` and `manufacturing_floor` PASS without any
  board geometry (`rule_engine.py:112-192`).
- **The docs say a BLOCKED profile stops `tools/route.py`; the code does not** [INFERRED from grep]. The
  docs are `EVIDENCE_AND_MATURITY_POLICY.md` and `RULE_ENGINE_ARCHITECTURE.md:77`. In code only
  `context.py:339` reads the profile status. The router gates on `validate_electrical_inputs`
  (`router.py:450-451`).
- **Strictness-key bugs** [OBSERVED]. `max_harmonic` matches `max_` first, so it is read as
  smaller-is-stricter. `temperature_rise` is listed as larger-is-stricter, which looks backwards
  [INFERRED]. Dropping a losing rule drops all of its other parameters too.
- **Unwired code** [OBSERVED]. `provenance.check_rule` and `can_transition` run only in tests, and
  archetype/overlay `rule_ids` are never used for composition.
- **A probable false-positive overlay** [INFERRED]. `motor_drive` was added to the UPS half-bridge.

### 3.3 EAK today
- `crates/eak-intelligence/src/domain/{registry,classify,features}.rs` with
  `data/desk_domains.json` (generated by `scripts/gen-desk-domains.sh` from DESK; ADR-0011)
  [OBSERVED]:
  - The registry has the 100 archetypes and 17 overlays.
  - The classifier is the same algorithm in Rust.
  - `Maturity` plus `Ceiling` cap guidance by the DESK label. Only VERIFIED_AGAINST_SOURCE and
    LIVE_VERIFIED may be shown as requirements.
  - Tests in `crates/eak-intelligence/tests/domain.rs`: `registry_contains_every_desk_class`,
    `mains_involvement_is_never_resolved_silently`, `empty_design_is_unclassified_not_guessed`,
    `guidance_is_capped_by_the_desk_maturity_label`, `results_always_state_what_is_not_implemented`.
- `systems/domains.rs` maps DESK domains and overlays to EAK engineering domains, with a `high_risk`
  flag. `systems/interaction.rs` holds cross-domain interaction rules (names only; they compute nothing).
- **Missing:** the 66-rule catalogue as data, the rule engine, the composer, the profile compiler, and
  the evidence-transition validator. EAK result vocabularies are split, and none of them has
  `UNSUPPORTED` or `DEFERRED`:
  - `eak-validation/src/state.rs` `CheckState`;
  - `gates.rs` `GateState`;
  - `eak-engineering/src/verify.rs` `Verdict`;
  - `eak-manufacturing` `PackageStatus`.

### 3.4 Proposed EAK-native schema
- **`RuleRecord`**, stored as versioned data in `eak-intelligence/data/rules/*.json` and generated from
  DESK `rule_sets` the way `desk_domains.json` is:
  - `id`, `version`, `scope{archetypes, overlays, families}`, `category`, `severity`, `blocking: bool`;
  - `applicability` (the same operator set);
  - `inputs: [{key, unit: Unit, required: bool, on_missing: NotEvaluated|Fail}]`. This makes DESK's
    fail-closed list explicit per input: BAT-CHEM-130, MOTOR-PEAK-140, MAINS-INS-163, MAINS-DECL-160;
  - `evaluator: {crate::path, version}` or `Advisory`;
  - `parameters` (typed, with units), `sources: [ReferenceId]`, `evidence: EvidenceState`;
  - `enforcement{subsystem, mode}`, `human_review_required`, `rationale`.
- **`EvidenceState`**: DRAFT, ADVISORY, IMPLEMENTED, OFFLINE_TESTED, VERIFIED_AGAINST_SOURCE,
  LIVE_VERIFIED, NEEDS_HARDWARE_VALIDATION, BLOCKED. Keep DESK's transition table
  (`desk/domain_intelligence/provenance.py:16-34`): the two VERIFIED states are human-only, and nothing
  may be VERIFIED_AGAINST_SOURCE while the table flag is false. Run that check in the library loader,
  not only in tests.
- **`RuleOutcome`**: PASS, FAIL, NOT_EVALUATED, DEFERRED, ADVISORY, UNSUPPORTED, BLOCKED.
  - PASS is allowed only from an evaluator whose evidence is ≥ IMPLEMENTED **and** which evaluated
    actual design objects.
  - A "derivable" result is reported as `DERIVED_LIMIT` (the limit is known) and never as PASS.
  - **A BLOCKING rule that is NOT_EVALUATED, DEFERRED-unrun or UNSUPPORTED makes the profile BLOCKED.**
  - Map onto the brief's §8.2 set (PASS/FAIL/UNKNOWN/NOT_RUN/UNSUPPORTED/BLOCKED) with one shared enum
    in `eak-validation`, used by checks, gates and verify verdicts. This changes a shared contract, so
    it needs an ADR.
- **Composition**: port the stricter-wins algorithm, but declare strictness **per parameter in the rule
  data** instead of inferring it from key names, which removes the `max_harmonic` bug. Keep "a more
  severe rule is never dropped" and "unknown strictness keeps both and requires review". Drop
  individual parameters, not whole rules.
- **`DomainProfile`**, stored per revision as a validation artifact, not as IR: classification,
  overlays, rule-set version hash, evaluations (each linked to affected object ids), conflicts,
  blocking, missing information, and status.

**Tests to port:**
- `test_rule_composition.py`: `test_more_severe_rule_is_never_dropped_even_if_looser`,
  `test_unknown_strictness_requires_review_and_keeps_both`;
- `test_rule_engine.py`: `test_unknown_evaluator_is_unsupported_not_pass`,
  `test_charge_voltage_fails_closed`;
- `test_profile_compiler.py`: `test_mains_without_declared_rails_is_blocked`;
- `test_rule_provenance.py`: `test_no_rule_claims_verification_against_source_yet`;
- a new test: a BLOCKING rule that is NOT_EVALUATED blocks.

## 4. Calculation and evidence provenance

### 4.1 DESK calculation modules [OBSERVED]

| Module | Computes | Standard cited | Fail-closed behaviour |
|---|---|---|---|
| `desk/ipc.py` | trace width (A = (I/(k·ΔT^0.44))^(1/0.725), k 0.048 external / 0.024 internal); clearance band lookup (+ per-volt above 500 V); creepage via the insulation dataset; `required_spacing_mil` = max(clearance, creepage ≥ 10 V, fab minimum); via current | IPC-2221B, IEC 60664-1 | current ≤ 0 raises; IPC-2152 explicitly not implemented (`:11-12`) |
| `desk/insulation/{engine,tables,signoff}.py` | separate `ClearanceResult` and `CreepageResult`, type-checked so they cannot be swapped (`engine.py:163-178,552-557`); status VERIFIED-AGAINST-TABLE / PROVISIONAL / UNVERIFIED | IEC 60664-1:2020+A1:2025 | missing inputs are named, never defaulted (`:277-345`); no coating credit; the loader rejects untraceable or non-monotone data (`tables.py:130-283`); a compliance statement needs a sign-off matching dataset and digest (`signoff.py:79-95`), otherwise `NOT_A_COMPLIANCE_CLAIM` |
| `desk/spacing.py` | keeps `functional_clearance_mm`, `creepage_mm` (only across a declared `InsulationBarrier`), `fabricator_min_mm` and `pin_field_mm` apart; policy `CREEPAGE_AS_CLEARANCE` (default) vs `FUNCTIONAL` | as above | an unmeetable rule is a BLOCKING `EngineeringDecision` (`SPC-…`) with options, never lowered (`:131-141,233-248`; `routing/feasibility.py`) |
| `desk/profile.py` | per-net current/voltage with confidence VERIFIED / ESTIMATED / UNKNOWN (`:25-29`) | — | an undeclared power or HV net is UNKNOWN and sizing raises (`:221-262,370-412`) |
| `desk/power_distribution.py` | rail strategy PLANE / POLYGON / WIDE_TRACE / TRACE / BLOCKED; IR drop over the MST length | none (heuristic thresholds) | unknown current → BLOCKED; `ground.continuity_verified` hard-coded False |
| `desk/calc.py` | QA expressions in a sandboxed AST; SI-prefix parsing; min/max judgement | datasheet formula (free text) | no limit or an error → `ok=False`; **no real project uses `expr`** |
| `desk/quality.py` | 8 hard gates, states PASS / FAIL / UNKNOWN / NOT_RUN / STALE; evidence from another revision or profile hash is STALE | — | "manufacturing" passes if the minima are merely positive, and "electrical" if no current is negative (`:441-474`): hollow gates |

### 4.2 EAK today [OBSERVED]
- `crates/eak-engineering/src/ipc.rs`:
  - `trace_width_mil`, `trace_current_a`, `clearance_mm`, creepage rows;
  - `VERIFIED_AGAINST_STANDARD = false` (`:14`);
  - `power.rs` (typed `trace_width`, `voltage_drop`, `rail_drops`);
  - `impedance.rs` (IPC-2141A);
  - `calc.rs` (`junction_temp_c`);
  - `clearance.rs::required`, shared by DRC-002 and the router;
  - `verify.rs` (`Verdict` PASS/FAIL/UNKNOWN/BLOCKED; `voltage_spacing` follows the declared
    `Insulation`).
- `InsulationStandard` (`Ipc2221b | Iec60664_1`) and `Insulation` live in `crates/eak-core/src/ir.rs:1284-1291`.
- Provenance today:
  - `verify::Check{source, reason}`;
  - `eak-validation` `RuleInfo.source`;
  - `systems::CalculationRecord{inputs(source, knowledge), formula, method, assumptions, margin,
    CalcState, revision}`;
  - `eak-core/src/datasheet.rs` `DatasheetValue` / `FactLabel`.
- DC voltage drop has no DRC rule on `main`: `drc.rs:990` lists it as not implemented. Open PR #143 adds
  DRC-028 (https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/143).

### 4.3 Data whose standard provenance is unverified (DESK's own flags), not to be enforced as PASS
1. `desk/ipc.py:39` `verified_against_standard = False`. It covers every IPC-2221B and IEC 60664-1 value
   in `ipc.py`, which are "transcribed … as commonly published" (`:27-29`).
2. `desk/ipc.py:111-112` `CREEPAGE_BASIS = "PROVISIONAL / UNVERIFIED: … Table F.4 … as transcribed"`.
   It is attached to every spacing result (`spacing.py:126`).
3. `desk/insulation/data/iec60664-1_2020_a1_2025.provisional.json`: dataset
   `DESK-IEC60664-1-PROVISIONAL-2026-11-21`. Every cell is `PROVISIONAL` with `verified_by: null`, and
   the table number is itself unconfirmed. The clearance, rated impulse, altitude and CTI tables are
   **absent**, so clearance can never compute with the shipped data.
4. The same dataset's `creepage_reinforced_factor = 2.0` is PROVISIONAL ("Confirm the clause").
5. IPC-2221B Table 6-1 rows (`ipc.py:73-83`) have no per-cell provenance.
   - [INFERRED, to check] The A5/A6/A7 labels look swapped against the published table, and rows A5 and
     A7 are identical (`ipc.py:16-19,80,82`).
   - [INFERRED, to check] The PD1 creepage column looks like the printed-wiring basis while PD2 uses the
     general material-group basis. That mixed basis is what made UPS-1000 unroutable above 50 V
     (`live-test/ups-1000/ROUTING_BLOCKERS.md`).
6. `library/pcb_domains/references/references.json`: all 46 references are `REFERENCE_ONLY`. That
   includes the derived values used: IPC-2221B k/b/c constants, USB 90 Ω and Rd 5.1 kΩ, CAN/RS-485
   120 Ω, Ethernet 100 Ω/1500 V, and JLCPCB 5/4.02 mil.
7. Rule parameters with no verified source:
   - `BAT-CHEM-130.per_cell_v` (li-ion 4.2, li-hv 4.35, LiFePO4 3.65, NiMH 1.45, lead-acid 2.4,
     LTO 2.8);
   - `USBC-CC-150` (Rd 5.1 kΩ ±100, Rp 56k/22k/10k);
   - `LORA-BAND-110` region bands;
   - `MCU-RADIO-101.tx_peak_a_default 0.5`, applied silently (`rule_engine.py:214`);
   - the `DCDC-ISAT-120` ripple default 0.3 and boost efficiency 0.9;
   - `ROUTE-POLY-026` / `PWR-ROUTE-170` thresholds of 3 A and 10 A; ELEC-DROP 50 mV.
8. Fabricator presets (`profile.py:89-128`), each labelled version "2026.1" with no source: JLCPCB 2L
   5/4.02/12 mil, 4L 3.5/3.5/8, PCBway 4/4/8.
9. Heuristics with no standard:
   - via current uses the internal k on the barrel area, times 0.8 (`ipc.py:173-181`);
   - Cu resistivity 1.72e-8 Ω·m with no temperature coefficient;
   - power strategy thresholds of 2 A, 0.5 A, 60 mil and 2 % (`power_distribution.py:165-172`);
   - thermal spoke 8 mil/A;
   - stitching grid 2.54 mm;
   - net-name inference defaults of 3.3 V ESTIMATED and 0.2 A ESTIMATED.
10. **Clearance and creepage use the same |V1−V2| number** (`spacing.py:192`). Clearance should use a
    peak/impulse basis and creepage an RMS working voltage [INFERRED].
11. **Simulation models** (`docs/simulation/README.md:55-57`): `UNVERIFIED` cards cap a result at
    `PASS_WITH_CAVEATS`.
12. **All 100 archetypes:** `maturity.evidence` is "automated generation, pending human review"; 87 are
    DRAFT and 13 OFFLINE_TESTED. None is VERIFIED_AGAINST_SOURCE or LIVE_VERIFIED
    (`docs/domain_intelligence/EVIDENCE_AND_MATURITY_POLICY.md:34`).

**Rule for EAK:** any value from this list enters EAK only with `evidence ≤ OFFLINE_TESTED` and the
DESK flag carried verbatim. It may produce at most a `DERIVED_LIMIT`, `ADVISORY` or `UNKNOWN` result
unless a human verifies it against the standard text. EAK already does this for IPC
(`VERIFIED_AGAINST_STANDARD = false`). Extend it to per-cell provenance.

### 4.4 Proposed provenance schema (shared by calculations, rules and datasheet facts)

| Record | Fields |
|---|---|
| `TableDataset` | `id`, `standard{id, edition, clause, table}`, `cells[{key, value, unit, status: PROVISIONAL\|VERIFIED, verified_by?, verified_at?, note}]`, `content_sha256`; the loader refuses a VERIFIED cell without `verified_by`, and refuses non-monotone or missing provenance (DESK `tables.py` rules) |
| `CalculationRecord` (extends `systems::CalculationRecord`) | `id`, `evaluator{crate::fn, version}`, `formula_id`, `inputs[{name, value, unit, source: RequirementId\|DatasheetFact\|DesignObject\|TableCell, knowledge}]`, `assumptions[]`, `result{value, unit}`, `limit?`, `margin?`, `basis_status` (lowest evidence among its inputs and table cells), `design_revision`, `stale_if` (input revisions) |
| `ComplianceStatement` | allowed only through a `Signoff{dataset_id, digest, by, at}` matching the dataset, with both clearance and creepage VERIFIED_AGAINST_TABLE (DESK `signoff.py`); otherwise the output says `NOT_A_COMPLIANCE_CLAIM` |
| `EngineeringDecision` | `id` (content hash), `blocking`, `question`, `options[]`, `chosen?`, `by?`, `ledger_ref`; created when a rule is physically unmeetable (DESK spacing feasibility), never resolved by lowering the rule |

Spacing split to adopt from `desk/spacing.py`: functional clearance, creepage across *declared*
barriers only, the fabricator minimum, and the measured pin-field gap. Keep `FUNCTIONAL` vs
`CREEPAGE_AS_CLEARANCE` as a requirement-level choice (`safety_regulatory` dimension), confirmed by a
human. EAK has the beginning of this in `clearance.rs::required` + `verify::voltage_spacing`.

**Tests to port:**
- `test_insulation_engine.py`: `test_clearance_never_reads_creepage_data_and_vice_versa`,
  `test_each_missing_input_gives_unverified_and_is_named`, `test_no_output_says_compliant_without_signoff`,
  `test_loader_rejects_untraceable_data`;
- `test_reliability_spacing.py`: `test_creepage_rule_on_a_fine_pitch_pin_is_blocked_by_clearance_with_a_decision`,
  `test_rule_never_lowered_by_evaluation`;
- `test_hardware_reference_values.py`: `test_undeclared_high_voltage_net_fails_closed`.

## 5. Component evidence schema: family, candidate, MPN, datasheet, pin map, footprint

### 5.1 DESK chain [OBSERVED]

| Link | DESK | What is really checked |
|---|---|---|
| Family / role | `library/pcb_domains/component_families/families.json`: 206 families in 9 groups (`family_id`, `required_parameters`, `compatibility_requirements`, `datasheet_verification`, `placement_constraints`, `routing_considerations`, `relevant_archetypes`, `aliases`) | roles only, never MPNs; nothing checks a part against `required_parameters` |
| Candidate comparison | `docs/00b_COMPONENT_SELECTION.md:19-29`: an F1–F7 funnel (function → **recommended operating range, not abs-max** → margin 20–50 % → package → availability (stock ≥ 10× build, single source, NRND/EOL, assembler library) → cost → ecosystem); keep 2–3 candidates with a one-line rejection reason; table in `BOM_SELECTION.md` | markdown only; gate B "counts ✅" (`context.py:327-329`) |
| Passive derivation | `00b:50-74`: R power ≥ 2×; MLCC V ≥ 1.5–2× plus DC-bias C; inductor Isat ≥ (Iout + ΔI/2)·1.2–1.3; fuse hold ≥ 1.25×; Schottky Vr ≥ 1.5×; divider current ≥ 100× leakage | only the MLCC 1.5× rule is coded (`tools/bom.py:56-60`) |
| MPN identity | `component_intelligence/identity`: `desk_id` = SHA-256 of normalized manufacturer:MPN; `IdentityVerificationStatus` VERIFIED / PROVISIONAL / UNVERIFIED / CONFLICT; `LifecycleStatus` ACTIVE / NRND / EOL / OBSOLETE / UNKNOWN | `resolve()` always gives PROVISIONAL; VERIFIED and CONFLICT are never assigned |
| Library record | `library/devices.json` (162 records: `name`, `libraryUuid`, `uuid`, `footprint` (often empty), `lcsc`, `mfr`), written by `tools/resolve.py` from a live EasyEDA search; an exact match auto-saves, otherwise the human must `--pick` | "No invented parts": every MPN must resolve (`AGENTS.md` §G) |
| Datasheet evidence | `DatasheetReference{url, revision, sha256, …}` (URL only ever filled); the real evidence is prose such as `live-test/ups-1000/requirements/DATASHEET_FACTS.md` (doc number and revision, Fact/Value/Source with section and page, "NOT FOUND" when silent, variant caveats) | **no page, table or confidence fields**; the capability matrix marks datasheet verification MISSING |
| Spec provenance | `ComponentSpecification{…, extraction_method: DATASHEET_EXTRACTED\|SUPPLIER_CATALOG\|AI_EXTRACTED\|CALCULATED\|ASSUMED, verification_state: VERIFIED\|PROVISIONAL\|UNVERIFIED\|DISPUTED}` | no producer sets DATASHEET_EXTRACTED or VERIFIED |
| Symbol pin map | `library/pins/<MPN>.json` = `[{num, name}]` (156 files), produced by `tools/pins.py`: place the part off-sheet in live EasyEDA, read the pins, delete it (journaled); block `pins` must come from this file | pins come from the **EasyEDA symbol, not the datasheet**, so a wrong library symbol propagates [INFERRED] |
| Footprint / pads | `component_intel.FootprintIntegrityEngine` reads pads from the PCB dump | `verified=True` hard-coded (`component_intel.py:249`); unknown pads default to 20×20 mil; no symbol-pin ↔ pad check |
| Ratings | block `rating` text ("16V X5R", "Isat 2.6A") parsed into typed keys | self-declared, not from a datasheet |
| Block QA | `{item, formula, result, limit, source, pass}`, or computable `{expr, vars, unit, min, max, source}` | real blocks use only text QA with a hand-set `pass:true` (e.g. "commercial A TO VERIFY" with pass:true at `projects/UPS_1000/blocks/06_GATE_DRV.json:370-371`) |
| Lint / verify | `desk/spec.py:185-262`: every pin connected or NC, no undecided pins, single-pin nets only as ports, QA source required. `desk/netcheck.py` + `tools/verify.py`: live netlist == spec, no unexpected nets, `verified` ledger entry carrying a 12-char `spec_hash` → fresh / stale / never | real and fail-closed (`tests/test_truth_first.py::test_unknown_block_id_is_an_error_not_an_empty_pass`) |

**Fail-closed equivalence** (`procurement/equivalence.py`): verdict COMPATIBLE / INCOMPATIBLE /
**UNVERIFIED (missing data, never passes)**.
- Footprint: normalized package names with a conservative alias table; SOIC ≠ SOP and 0402 ≠ 1005 are
  deliberately kept apart. `pin_count` must be equal, pitch within ±0.01 mm, body within ±0.15 mm, and
  height may only be lower.
- Electrical: "at least" keys (V, I, P, Vds, Isat, Irms …, `*_max`); "at most" keys (tolerance, Rds_on,
  ESR, DCR, Vf, Iq, leakage …, `*_min`); every other key must be equal.
- Port this engine. **Do not port** the legacy `catalog/alternatives.py`: it reports PIN_COMPATIBLE on
  missing data and compares pins by list position.

### 5.2 EAK today [OBSERVED]
- `crates/eak-library/src/part.rs`: `PartKey` (manufacturer + MPN), `PartRecord`, `DatasheetRef` (with
  hash and revision), `Spec` / `SpecOrigin`, `DefLink`, `OfferSnapshot` (requires `retrieved_at_unix_s`),
  and `Provenance` (raw SHA-256). Also `mpn.rs`, and `source.rs` with `resolve`.
- `crates/eak-core/src/pinmap.rs::check_map`, plus PP-001..005 (`eak-validation/src/pin_pad.rs`) and
  FP-001..010.
- `eak-manufacturing/src/matching.rs` `MatchKind` (fuzzy is never approved).
- `eak-core/src/datasheet.rs` `DatasheetValue` / `FactLabel`.
- **Missing:**
  - alternates and equivalence (`eak-validation/src/sourcing.rs:435` lists second-source as not
    implemented);
  - candidate comparison with rejection reasons;
  - a passive-derivation evaluator;
  - a structured datasheet-fact locator (page / table / figure).
- The LCSC slot is a disabled placeholder (`eak-library/src/sources/lcsc.rs:13`).

### 5.3 Proposed EAK-native schema

| Record | Key fields | Rules |
|---|---|---|
| `ComponentRole` | `id`, `family_id` (DESK families as data), `block_id`, `requirement_ids`, `required_parameters[{key, unit, limit_kind, value?, derived_from?: CalculationRecord}]` | a role exists before any MPN (gate B) |
| `Candidate` | `role_id`, `part: PartKey`, `filters{F1..F7: Pass\|Fail{reason}\|Unknown}`, `margins[{key, required, offered, margin_pct, basis: RecommendedOperating\|AbsMax}]`, `status: Proposed\|Rejected{reason}\|Chosen{by, at}` | AbsMax may never satisfy an F2/F3 range check; rejected candidates are kept |
| `DatasheetFact` | `part`, `datasheet{sha256, revision, url}`, `locator{page, table?, figure?, section?}`, `key`, `value`, `unit`, `conditions`, `extraction: Manual\|Parsed\|AiExtracted`, `confidence`, `review: Unreviewed\|Confirmed{by}\|Disputed{note}` | `AiExtracted` stays Unreviewed until a human confirms it; "NOT FOUND" is a first-class value |
| `PinMapEvidence` | `part`, `symbol_ref`, `footprint_ref`, `pins[{number, name, function, pad?}]`, `source: Datasheet{fact ids}\|LibrarySymbol{id}`, `status: Unverified\|LibraryConsistent\|DatasheetVerified{by}` | a placed part needs ≥ LibraryConsistent (symbol pins ↔ pads, PP checks); safety or high-risk domains need DatasheetVerified |
| `Equivalence` | `design_part`, `candidate`, `footprint_verdict`, `electrical_verdict` (COMPATIBLE / INCOMPATIBLE / UNVERIFIED), `missing[]` | UNVERIFIED never substitutes |
| `BlockQa` | `item`, `evaluator` or `formula_id` + inputs, `limit`, `source: DatasheetFact\|Standard\|Requirement` | hand-set pass is not accepted; a text-only QA item is reported as `NOT_RUN` |

**Tests to port:**
- `tests/test_sourcing_i5.py::test_footprint_checks`, `::test_electrical_checks`;
- `tests/test_truth_first.py`;
- `tests/test_context.py::test_undecided_pin_and_single_net`;
- new tests: AbsMax cannot satisfy F2; an AI-extracted fact cannot reach Confirmed without a human.

## 6. Engineering workflow stages and dependency graph

### 6.1 DESK [OBSERVED]
```
Gate R (requirements: 10 dims + derivations, CONFIRMED)          docs/00 ; advisory in code
  └─► Domain classification → archetype + overlays → rules → profile   (AGENTS §F "DDIS"; desk.py domains)
        └─► Gate B (architecture parts → power/protection → interfaces → support ICs → passives → mechanical;
              F1–F7 matrix + reasons)                              docs/00b ; advisory ("✅" count)
              └─► per block, one at a time: G1 PLAN → G2 PARTS (resolve MPN) → G3 PINS (pin cache) →
                    G4 QA → G5 LINT → G6 BUILD → G7 VERIFY (live netlist == spec, spec_hash) → ACCEPT
                    next gate computed from state, desk/context.py:295-302; retry ceiling 2
                    └─► sheet finish: verify all + lint --final + EasyEDA ERC 0 + export netlist/BOM
                          └─► Placement (outline/mechanical → connectors → decap → thermal → routability) → freeze hash
                                └─► Routing (feasibility gate → chunked engine → best-SES checkpoint →
                                      import → widen → read-back → connectivity verdict FULL)
                                      └─► Copper (planes, pours, stitching, orphan check)
                                            └─► DRC converge → Release (offline + live gates, manifest)
```
- The task record order is requirements → architecture → bom → schematic_block → placement → stackup →
  routing → drc → erc → manufacturing → release (`desk/tasks.py:27-30`). Execution states are
  PENDING / RUNNING / SUCCEEDED / FAILED / BLOCKED / SUPERSEDED; verification states are NOT_RUN / PASS /
  FAIL / STALE.
- Physical order follows brief §7.2-I (`docs/04_PLACEMENT.md`, `docs/05_ROUTING.md`).

### 6.2 EAK today [OBSERVED]
EAK has at least five overlapping stage models:
- `eak-intelligence/src/systems/lifecycle.rs:19` `Stage`: 13 stages from ProductUnderstanding to
  ReleaseReview;
- `eak-harness/src/agent_task.rs:34` `Phase`;
- `eak-runtime/src/status.rs:41` `phase_of`;
- `apps/desktop/src/shell/journey.ts` `StageId`: 10 stages;
- `eak-harness/src/task_graph.rs`: a DAG in which PCB work requires the schematic and netlist checks.

`design_apply` is refused while the `hv-isolation-undefined` or `domain-unresolved` gate is open
(`eak-harness/src/design.rs:294,1295-1298`).

### 6.3 Proposal
Use one `WorkflowStage` enum in `eak-core` or `eak-validation`, consumed by lifecycle, harness phases and
the journey bar:

`Requirements → Classification → Architecture/BOM (gate B) → BlockPlan → per-block {Parts, Pins, QA,
Lint, Build, Verify} → SchematicComplete (ERC) → Mechanical/Outline → Placement → Routing → Copper →
Verification (DRC + engineering checks) → ReleaseCandidate → Released`.

Each stage has these properties:
- **Typed entry predicate.** It is computed from IR plus reports, never from a stored flag. This follows
  DESK's "status field cannot fake progress" rule (`tests/test_context.py::test_status_field_cannot_fake_progress`).
- **Explicit requirement dependencies** (§2.3 table).
- **Staleness** when an upstream revision changes.

The block loop maps onto EAK commands:

| DESK gate | EAK equivalent |
|---|---|
| G2 | the part resolves to an `eak-library` `PartRecord` |
| G3 | `PinMapEvidence` ≥ LibraryConsistent |
| G4 | `BlockQa` evaluated by `eak-engineering` |
| G5 | ERC plus the spec lint |
| G6 | `Command` batch through `Engine::execute` |
| G7 | netlist read-back equals the block plan, with a spec hash stored in the report |

## 7. Approval, single writer, rollback and read-back

### 7.1 DESK [OBSERVED]

**Lock** (`desk/ops.py`)
- One file per project: `state/write.lock`, created with O_EXCL. It holds id, op, target, pid,
  pid_start_time and ts.
- A lock is stale after a 1800 s TTL or when its holder PID is dead. PID reuse is detected from the
  process start time on Linux, Windows and macOS.
- Reclaim goes through a guarded rename race (F-9 fix), and a lock is released only by its owner.
- `write_op` refuses while an earlier op on the same target is pending or dirty.
- Classifications: READ_ONLY / IDEMPOTENT / NON_IDEMPOTENT / VALIDATION.
- Tests: `test_mutation_safety.py::test_stale_lock_atomic_reclaim`,
  `test_m1_reliability.py::test_two_threads_exactly_one_holder`.

**Approval**
- Ids come from `ops.record_approval` (op, target, granted_by, how, token, reason, ts),
  `ops.cli_approval` (an operator ran the tool), or a rework `RW-APR-` token.
- `DESK_MUTATION_POLICY=require` refuses unapproved non-idempotent ops before locking. A typo in the mode
  fails closed to `require` (`test_mutation_policy.py::test_unknown_policy_value_fails_closed_to_require`).
- **Weakness:** `MutationPolicy.check` only tests that the approval is non-empty (`ops.py:212`). Ids are
  never looked up or bound to the command.
- **Weakness:** rework tokens are a SHA-1 of the time, and their revision hash is never re-checked.
- **Weakness:** one approval covers a whole routing run (`routing/pipeline.py:78`).
- Captain sign-off covers destructive edits, clearance waivers, `spacing_policy: FUNCTIONAL`,
  releases, and the rework high-risk list (`docs/rework/08_APPROVAL_AND_SAFETY.md:32-42`).

**Window/project binding** (`desk/bridge.py`)
- A mutating script must have a verified window binding by project uuid; an unbound mutation is refused.
- After a reconnect rotates the window id, DESK rebinds only when exactly one window matches, and never
  by name.
- A mutation timeout or a post-send transport error is never retried; a refused connection proves the
  script was not delivered.
- Tests: `tests/test_reliability_sessions.py`.

**Read-back contract** (`desk/reliability/mutation.py`)
- Sequence: before read-back → precondition → identity → journal begin → execute → identity → settle →
  read-back → named verifier.
- Outcomes: EXECUTED_VERIFIED, FALSE_SUCCESS (true return, unchanged state), PARTIAL, WRONG_DOCUMENT,
  GATEWAY_NOT_EXECUTED, NOT_EXECUTED, EXECUTED_RESPONSE_LOST, EXECUTED_STALE_READBACK, UNSUPPORTED,
  UNKNOWN. Exit code 0 only when verified.
- Verification contracts: `count_increased`, `ids_absent`, `ids_present` (with a planned count, so an
  empty list cannot pass), `placements`, `pin_net_map`, `copper_counts`, `pour_present` …; an unknown
  contract fails.
- An API capability registry records the API calls whose result cannot be trusted:
  - `importChanges` and `clearRouting` open a DIALOG;
  - `PrimitiveLine.delete` returned true while 33 of 1360 tracks survived;
  - `modifyPcbName` renamed a different PCB;
  - SES import rewrote wire widths.
- Tests: `tests/test_reliability_api.py::test_import_changes_true_without_components_is_false_success`,
  `::test_delete_true_with_survivors_is_partial`.
- **Coverage gap:** only `pcb_place`, `pcb_rules`, `copper_fill` and `stitch_orphans` use the contract
  (`tests/test_mutation_kernel.py::test_the_four_tools_go_through_the_readback_contract`). `build`,
  `wire`, route import, rip-up, widen and repour do not. An unverified PARTIAL or UNKNOWN outcome is
  journalled as `end, verified:false`, so `recover` never sees it, and `reconcile()` has no caller.

**Rollback**
- There is **no real undo** (capability matrix #39 MISSING).
- The routing best-SES checkpoint is real (`desk/routing/checkpoint.py`).
- `runtime/checkpoints` keeps metadata only, pruned to 10.
- The rework executor is SIMULATED, on an in-memory snapshot, and is never VERIFIED.
- The Supervisor (evidence → diagnosis → policy-gated recovery → verify → incident) has no production
  caller.

### 7.2 EAK today [OBSERVED]
**Engine** (`eak-runtime/src/engine.rs:368` `Engine::execute`)
- Pipeline: revision check (`StaleRevision`) → `check_policy` → apply to a copy → integrity → diff →
  pending journal → atomic write → committed journal → event.
- `CommandRequest{expected_revision, actor, command, approval{max_class}}`.
- `OperationClass`: ReadOnly < ReversibleLocal < EngineeringImpacting < Destructive. A batch takes the
  class of its riskiest step.
- Undo/redo works from before/after deltas; the project lock is `store.rs:240 acquire_lock`;
  interrupted ops are reconciled on open.
- Agent modes: `AgentMode` Manual / Assisted / Autonomous, where Autonomous never covers Destructive
  (`eak-intelligence/src/agent.rs:35`).
- `task_approval.rs` requires a named approver at an unchanged revision. The harness `task_journal.rs` is
  a SHA-chained journal with idempotency keys for side effects.
- Tests in `eak-runtime/tests/engine.rs`: `agent_without_approval_is_refused`,
  `stale_revision_is_rejected_never_applied`, `undo_redo_round_trips_every_command_kind`,
  `interrupted_operation_is_reconciled_on_open`.

EAK is already stronger than DESK on single-writer and rollback: compare-and-swap revisions instead of
path-string targets, and real undo instead of no undo.

### 7.3 What to adopt and what to avoid

**Adopt from DESK:**
1. **Read-back as a verifier per command.** Each mutating `Command` declares a postcondition (DESK
   contract kinds: ids_present with a planned count, ids_absent, placements, pin_net_map,
   copper_counts). `Engine::execute` returns `verified` only when the postcondition holds on the
   committed IR. This matters most for long-running engines (autoroute, pour, import):
   `CommandResponse.validation_note` already says acceptance is not validation, and EAK's
   `connectivity_proof` after AutoRoute (`command.rs:1271,3261`) is the model to follow.
2. **Outcome states for external bridges.** Any future EasyEDA/KiCad import/export bridge needs FALSE_SUCCESS / PARTIAL /
   NOT_EXECUTED / UNKNOWN. **PARTIAL and UNKNOWN stay pending** and block the target until reconciled;
   this fixes DESK's gap.
3. **Bound approvals.** An approval is `{id, command_hash, project_revision, class, approver, expiry}`.
   It is checked and consumed by the engine, never accepted for being non-empty. Approval is granular:
   one per destructive sub-step, not one per routing run.
4. **Engineering decisions instead of rule relaxation.** See §4.4.
5. **Captain sign-off list:** destructive edits, waivers, spacing policy, release, high-risk domains.
   This matches brief §8.6 and EAK's `high_risk` domains.

**Avoid:**
- DESK's EasyEDA window/bridge model. EAK owns the native IR. A compatibility bridge must be explicit,
  revision-safe and independently validated (brief §7.3).
- Path-string journal targets.
- Prefix-accepted approval tokens.

## 8. Sourcing and manufacturing status semantics

### 8.1 DESK [OBSERVED]

**Offer `availability_source`**
- Values: `live_api` | `agent_evidence` | `fixture` | `seed` | `unverified` (default). Only `live_api`
  and `agent_evidence` count as verified sources (`suppliers/models.py:15-20`).
- An evidence record (`suppliers/evidence.py`, schema `desk.sourcing.evidence/1`) must have:
  - a known supplier;
  - an MPN;
  - an **https URL on the supplier's own domain**;
  - `verified_at` in ISO 8601 **with a timezone**, and no more than 5 minutes in the future;
  - stock that is an integer ≥ 0 or null, and price > 0 or null, with a currency whenever a price is
    present.

  Invalid records are rejected, never coerced, and the newest record wins.

**Inventory state** (`procurement/inventory.py:26-179`)
- Values: IN_STOCK_CONFIRMED, LOW_STOCK (< 2× required), OUT_OF_STOCK, BACKORDERED, DISCONTINUED (wins
  over residual stock), STOCK_UNKNOWN, PRICE_UNAVAILABLE.
- STOCK_UNKNOWN covers four cases: an unverified source, no timestamp, a future timestamp, or **older
  than 7 days**.
- Unknown lifecycle text stays UNKNOWN and is never ACTIVE.
- Tests (`tests/test_sourcing_i5.py`): `test_unverified_is_never_in_stock`,
  `test_stale_future_and_missing_timestamps`, `test_fixture_offers_are_stock_unknown`,
  `test_backorder_only_with_client_flag`.
- Inconsistency: V4 verification calls the same "older than 7 days" case STALE.

**Selection and BOM verdicts**
- Line status: SELECTED / SELECTED_ALTERNATIVE / SELECTED_BACKORDER / UNRESOLVED, each with its reason.
- The ranking key is deterministic: designed MPN → stock tier at a preferred supplier → confirmed over
  low → lifecycle → price → lead time → supplier.
- BOM verdict is VERIFIED (every line in stock and priced, with supplier, URL and timestamp) or
  INCOMPLETE. This is **supplier evidence only, not hardware** (`00b:93-94`).

**Supplier reality**
- LCSC: a real keyless GET by C-number. There is no keyless MPN search, and that is recorded as a
  limitation. Without credentials it falls back to a fixture catalogue tagged `fixture`.
- Mouser: a stub.
- Robu and JLCPCB parts: evidence-only (Cloudflare / robots.txt).
- JLCPCB quote and order: local approximations with a mock order. `order_guard` tokens are
  `CAPTAIN-APPROVE-<hex>` (300 s, single use), but `confirm_token` has no caller authentication
  [INFERRED].
- Assembly class: Basic / Preferred / Extended / Unsupported / UNKNOWN. A heuristic class is labelled
  unverified.

**Release** (`tools/release.py`, `desk/release.py`)
- Offline gates: lint --final, ERC, every block verified **fresh**, no pending ops.
- Live gates: ERC 0, DRC 0, PCB designators equal spec designators.
- Exports are written to `.partial`, then cross-checked: BOM designators against specs, PnP complete,
  and the Gerber zip containing copper, outline, Excellon M48, mask ≠ paste, and inner layers.
- The result is an immutable revision directory with `MANIFEST.json` (sha256 per file, gate evidence,
  spec hashes, git commit), and `--check` re-verifies it.
- **Weaknesses:** `--skip-drc-reason` records DRC as `ok: true` (`tools/release.py:47-48`); there is no
  sourcing or BOM gate; Captain approval is not consulted.
- Rework readiness emits `READY_FOR_MANUFACTURING` (`desk/rework/readiness.py:22`). **EAK forbids that
  wording.**

### 8.2 EAK today [OBSERVED]
- `eak-library` `OfferSnapshot` requires `retrieved_at_unix_s`. The Distributor client uses signed
  OpenAPI with `fetched_at_unix_s`. The sourcing state progresses search-hit → datasheet-matched →
  verified.
- `eak-manufacturing` `PackageStatus` is `DRAFT | INTERNALLY_VERIFIED_RELEASE_CANDIDATE` (`lib.rs:40`).
  The `Manifest` binds revision, `input_sha256` and per-file sha256. `verify_package_for` rejects a
  package from another revision or one that was tampered with. `statement_of` never claims
  manufacturing-ready.
- Tests: `release_integrity.rs::failing_and_unknown_checks_keep_the_package_draft`,
  `::packaging_refuses_a_package_from_another_revision`;
  `parse_back.rs::two_layer_reference_board_reads_back_as_the_database`.

### 8.3 Proposal

**Sourcing.** Port DESK's 7-state inventory classifier and evidence-record validator verbatim as
semantics:
- an `availability_source` whitelist;
- a timezone-aware timestamp;
- a staleness window as config (default 7 days, giving STOCK_UNKNOWN, with one meaning everywhere);
- fixture data is never stock-confirmed.

LCSC stays first priority with a provider boundary. Record the "no keyless MPN search" limitation as
DESK did.

**Release ladder** (brief §8.7). Each status is a separate typed state with an evidence predicate:

| Status | Requires |
|---|---|
| `DRAFT` | files generated |
| `VERIFIED_FOR_REVIEW` | every applicable gate PASS at this revision (today's `INTERNALLY_VERIFIED_RELEASE_CANDIDATE`); no `NOT_RUN`/`UNKNOWN` on a required gate; sourcing VERIFIED or explicitly accepted |
| `ENGINEERING_REVIEWED` | a named human review record bound to revision + manifest hash; high-risk domains need their own sign-off |
| `PROTOTYPE_FABRICATED` | a fab order/receipt record bound to the manifest (human-entered) |
| `MEASURED` | measurement records against requirement ids (human-entered) |
| `RELEASED` | all of the above for the release scope plus an explicit release approval |

A skipped gate is recorded as `NOT_RUN` (never ok). The manifest also binds the rule-set version and the
`DomainProfile` hash.

## 9. Master mapping: DESK module → concept → EAK equivalent → missing → proposed native → tests and evidence

| DESK module / path | Concept / contract | Current EAK equivalent | Missing EAK capability | Proposed native implementation | Tests / evidence to carry |
|---|---|---|---|---|---|
| `docs/00_REQUIREMENTS.md`, `templates/REQUIREMENTS.md`, `desk/context.py:82-92,323-332` | 10 requirement dimensions, CONFIRMED/PROVISIONAL/TBD, gate R | `eak-core ir.rs:117` `Requirement{id,text}`; `eak-intelligence systems/model.rs` (computed); `eak-harness design.rs` `DesignSpec` (side file) | stored typed requirement, statuses, scoped gate, owner/timestamp, power budget/modes/interfaces/GPIO tables | `RequirementRecord` in IR plus commands (§2.3); ADR | `test_requirement_rows_indexed`, `test_status_field_cannot_fake_progress`; UPS_1000 scoped-gate replay |
| `desk/domain_intelligence/classifier.py`, `library/pcb_domains/archetypes,overlays,taxonomy.json` | archetype/overlay classification | `eak-intelligence/src/domain/*` + `data/desk_domains.json` (ADR-0011) | none for the classifier; regenerate on DESK updates | keep; add `current_a` scoring that DESK lacks | `tests/domain.rs`; DESK `test_requirement_classifier.py` |
| `library/pcb_domains/rule_sets/*`, `desk/domain_intelligence/rule_engine.py, rule_composer.py, profile_compiler.py` | 66 rules, evaluators, stricter-wins composition, compiled profile | none (`classify.rs:16` "listed, not executed") | rule data, engine, composer, profile | `RuleRecord` data + Rust evaluators in `eak-validation`/`eak-engineering`; `DomainProfile` per revision (§3.4) | `test_rule_composition.py`, `test_rule_engine.py`, `test_profile_compiler.py`; new: BLOCKING+NOT_EVALUATED blocks |
| `desk/domain_intelligence/provenance.py`, `docs/domain_intelligence/EVIDENCE_AND_MATURITY_POLICY.md` | 8-state evidence ladder, transitions, enforcement coupling | `domain/registry.rs` `Maturity` + `Ceiling` | transition validation, enforcement coupling, loader check | `EvidenceState` + loader validation | `test_rule_provenance.py::test_transitions`, `::test_no_rule_claims_verification_against_source_yet` |
| `desk/ipc.py` | IPC-2221B width/clearance, IEC creepage | `eak-engineering ipc.rs` (`VERIFIED_AGAINST_STANDARD=false`) | per-cell provenance; IPC-2152 (both lack it) | `TableDataset` (§4.4) | `test_ipc_models.py::test_reference_widths`, `::test_unknown_current_is_an_error_not_a_width` |
| `desk/insulation/*`, `desk/spacing.py`, `desk/routing/feasibility.py` | clearance vs creepage vs fab minimum; barriers; sign-off; feasibility decision | `eak-engineering clearance.rs`, `verify::voltage_spacing`, `eak-core` `Insulation` | typed separate results; barrier model; feasibility gate → `EngineeringDecision`; sign-off | port the concepts with a dataset loader; **do not** import DESK's provisional values as verified | `test_insulation_engine.py` (10 named in §4.4), `test_reliability_spacing.py`; UPS-1000 `ROUTING_BLOCKERS.md` |
| `desk/profile.py`, `desk/power_distribution.py` | per-net current/voltage confidence; unknown → BLOCKED; rail strategy, IR drop | `eak-engineering power.rs` `voltage_drop`, `rail_drops`; DRC-027 via current; DRC-028 drop in open PR #143 | `UNKNOWN` current blocks sizing (EAK: confirm); strategy selection | net current from `RequirementRecord.power_budget`; `UNKNOWN` → BLOCKED | `test_power_distribution.py::test_undeclared_rail_is_blocked_not_sized`, `test_profile_hardening.py` |
| `desk/quality.py`, `docs/verification/ENGINEERING_EVIDENCE_MODEL.md` | 8 hard gates, PASS/FAIL/UNKNOWN/NOT_RUN/STALE | `eak-validation gates.rs` `GateState` + `report.rs` (`not_implemented`, `limitations`) | STALE as a state; one shared outcome enum | unified outcome enum (§3.4) | `test_quality_fail_closed.py::test_high_score_never_overrides_hard_gate_failure`; EAK `report_goes_stale_on_new_revision` |
| `desk/calc.py`, block `qa[]` | sandboxed QA formulas | `eak-engineering calc.rs`, `systems::CalculationRecord` | block QA bound to evaluators; reject hand-set pass | `BlockQa` (§5.3) | `test_core.py::test_divider_pass_fail_and_safety` |
| `docs/00b_COMPONENT_SELECTION.md`, `BOM_SELECTION.md` | F1–F7, keep rejected, passive derivation | `eak-manufacturing matching.rs` (MPN match only) | role → candidates → filters → margins → rejected list; passive evaluators | `ComponentRole`, `Candidate` (§5.3); passive rules as `eak-engineering` evaluators | new tests (AbsMax cannot satisfy F2) |
| `desk/component_intelligence/identity/*` | canonical identity, lifecycle | `eak-library part.rs` `PartKey`, `PartRecord` | lifecycle enum, conflict state | extend `PartRecord` | `test_ci_identity.py` |
| `library/pins/*`, `tools/pins.py`, `desk/spec.py` lint, `desk/netcheck.py` | pin map from library, every pin mapped or NC, live netlist == spec | `eak-core pinmap.rs`, `eak-validation pin_pad.rs`, ERC | datasheet-sourced pin evidence status | `PinMapEvidence` (§5.3) | `test_context.py::test_undecided_pin_and_single_net`, EAK `unmapped_pin_missing_pad_and_unknown_pin_block_placement` |
| `live-test/*/requirements/DATASHEET_FACTS.md` | datasheet facts with page and section | `eak-core datasheet.rs` `DatasheetValue`, `FactLabel` | locator (page/table), review state, "NOT FOUND" | `DatasheetFact` (§5.3) | — |
| `desk/component_intelligence/procurement/equivalence.py` | COMPATIBLE/INCOMPATIBLE/UNVERIFIED | none (`sourcing.rs:435`) | alternates | port the semantics | `test_sourcing_i5.py::test_footprint_checks`, `::test_electrical_checks` |
| `desk/component_intelligence/procurement/{inventory,selection}.py`, `suppliers/evidence.py` | 7 inventory states, evidence records, ranking | `eak-library OfferSnapshot`, `eak-manufacturing distributor_api.rs`, `sources/lcsc.rs` (disabled) | inventory classifier, evidence validator, ranking with reasons | port (§8.3) | `test_sourcing_i5.py` (21 tests) |
| `desk/manufacturing/jlcpcb/*` | assembly class, rotation offsets, quote, order guard | `eak-manufacturing fabricator.rs` (BOM/CPL/DFM) | assembly-class evidence | port the class semantics; **do not port** the quote, order or catalogue data | `test_jlcpcb_integration.py::test_order_guard_blocks_unauthorized_order` |
| `tools/release.py`, `desk/release.py` | gated release, cross-checks, immutable manifest | `eak-manufacturing` `Manifest`, `verify_package_for`, `release_gates_with_report` | status ladder; BOM/sourcing gate; NOT_RUN for skips | §8.3 ladder | `test_core.py::test_gerber_and_manifest`, `test_engineering_data_i2.py::test_paste_mask_is_not_solder_mask`; EAK `release_integrity.rs` |
| `desk/ops.py`, `desk/bridge.py` | single-writer lock, approvals, journal | `eak-runtime engine.rs`, `store.rs acquire_lock`, `OperationClass`, `Approval` | bound approval records; per-command verifier | §7.3 | `test_mutation_policy.py`, `test_m1_reliability.py`; EAK `tests/engine.rs`, `tests/lock.rs` |
| `desk/reliability/{mutation,api_registry,models}.py` | read-back contracts, execution states, capability registry | `CommandResponse.validation_note`; AutoRoute `connectivity_proof` | postcondition per command; outcome states for bridges | §7.3 items 1–2 | `test_reliability_api.py` (21), `test_mutation_kernel.py` |
| `desk/rework/*` | baseline → audit → plan → approve → execute → report | `eak-harness/src/rework.rs` (re-hashes protected objects) | real (non-simulated) execution via commands | rework plan = `Command` batch proposal + approval + verify | `test_rework_executor.py::test_no_result_is_ever_verified_and_journal_records_no_board_mutation` (shows the anti-pattern) |
| `desk/ledger.py`, `desk/decision_graph.py`, `desk/recovery_case.py` | append-only ledger: source required, corrections supersede, spec-hash staleness | `eak-runtime journal.jsonl`, `eak-harness task_journal.rs` (SHA-chained), WEFT conflicts/review in open PR https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/130 (ADR-0053) | engineering decision/fact ledger tied to requirement ids | `LedgerEntry{type: decision\|fact\|assumption\|correction, source (required), refs, revision}` in the project store, written by the engine | `test_context.py::test_source_required_and_corrections`, `test_decision_history.py` |
| `desk/context.py`, `tools/ctx.py` | token-budgeted context packs by structure (block → reqs → nets → pins) | `eak-intelligence knowledge` (BM25), `intel_*` tools | structured "next gate" pack for agents | harness tool `context_pack{block}` computed from IR | `test_context.py::test_block_pack_cheaper_than_raw_reading`, `::test_budgets_respected` |
| `desk/routing/{checkpoint,connectivity,pipeline}.py` | best-result checkpoint; verdict from connectivity read-back only | `command.rs` `connectivity_proof`, `verify_routing` | best-so-far checkpoint for long routes | keep EAK's proof; add checkpointing if routes become long-running | `test_routing_chunked.py`, `test_reliability_routing.py`; EAK `autoroute_multilayer.rs` |
| `learning/`, `desk/learn/*` | incidents and lessons, ≥2-session corroboration, human-gated | — | — | development-process knowledge, not product; see §10 | `test_learn.py` |

## 10. Reusable knowledge versus code not to port

**Reuse as knowledge** (re-expressed as EAK data, schemas and Rust):
- The 10 requirement dimensions and the gate R derivations.
- The domain taxonomy, 100 archetypes, 17 overlays and 206 families. The first three are already
  ported; the families are not.
- The 66-rule catalogue's ids, scope, severity, applicability, fail-closed inputs and rationale,
  carried **with their evidence state**.
- The stricter-wins / never-drop-the-more-severe composition principle.
- The 8-state evidence ladder.
- The F1–F7 selection funnel and the passive margin rules.
- Equivalence and inventory semantics, and the evidence-record validation rules.
- The clearance / creepage / fab-minimum / pin-field split, and the engineering-decision pattern.
- The block G1–G7 loop with computed next gate and spec-hash staleness.
- Read-back contract kinds, and the EasyEDA API false-success lessons (if a bridge is ever built).
- The release cross-checks.
- The live-test protocol: frozen hashes, stage budgets, failure register, independent examiner.

**Do not port** (code or data):
- Any Python. This is an EAK non-negotiable.
- DESK's EasyEDA bridge, window binding and drawing code (`desk/bridge.py`, `tools/build.py`,
  `desk/draw.py`). EAK owns its IR.
- Freerouting orchestration (`desk/routing/orchestrator.py`, chunking). There is no mandatory
  Freerouting in EAK.
- `catalog/alternatives.py`: it passes on missing data.
- `verification/engine.py` V2/V3: they pass by default.
- `component_intel.py`: it hard-codes `verified=True`.
- `specifications/engine.py`: it parses "1M" Ω as 1 mΩ (`:89`).
- JLCPCB quote, catalogue and order mock data. The C347222 data conflicts, and prices are
  approximations.
- The Mouser stub, and the LCSC fixture catalogue as default data.
- IPC and IEC table *values* as verified facts. They are PROVISIONAL (§4.3).
- Rework executor and connection: simulated and fail-open.
- `READY_FOR_MANUFACTURING` and any "manufacturing-ready" wording.
- The Captain/First-Mate persona rules in DESK `AGENTS.md` §A. These are operator UX for DESK's agents
  and outside EAK's product scope.

## 11. Real workflows to use for acceptance testing

Each workflow below is a replayable EAK fixture. Inputs come from the DESK records; EAK builds through
`Engine::execute` only.

1. **ESP32-S3 USB dev board, frozen-requirements timed run.**
   - Source: `live-test/esp32-s3-plotus{,-run2}/requirements/{REQUIREMENTS,ACCEPTANCE_CRITERIA,DESIGN_PARAMETERS}.md`
     and `FROZEN_HASHES.txt`.
   - Inputs: 2 layers, 74 × 44 mm, rounded corners, about 40 parts.
   - Gates, each Must Pass:
     - S1 schematic, live netlist equal to spec, ERC 0;
     - S2 placement: C5 ≤ 2 mm from the module, rounded outline;
     - S3 routing: 43/43 nets, USB D+/D- about 90 Ω, no stubs, GND routed in S3;
     - S4 copper: no GND opens, a via is not trusted to join a fill;
     - S5 manufacturing: Gerber, drill, BOM with LCSC, CPL.
   - Evidence: timing per stage, a failure register `{id, stage, operation, expected, actual,
     root_cause CONFIRMED|UNCONFIRMED, recovery, time_lost_min, final_status}`, and an independent
     examiner that parses the raw DRC itself.
   - Baselines: run 1 FAIL 50/100 (5 GND opens; DRC parser printed 0 for 174 real clearance errors,
     F-015). Run 2b PARTIAL PASS 81/100 (USB about 153 Ω, not 90; a 0.8 mm stub; 17 ghost texts left
     by `clear_block`).
   - Overlap with EAK: open PR #142 (https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/142)
     adds a USB-device fixture.
2. **UPS_1000 LV half-bridge, scoped gates and an unroutable-spacing decision.**
   - Source: `projects/UPS_1000/{REQUIREMENTS.md, requirements.json, design_profile.json, domain_validate.json}`
     and `live-test/ups-1000/*`.
   - Expected: gate R passes for `lv_prototype` and stays blocked for `system_hv` (M-01..M-13).
   - The classification must show REVIEW_REQUIRED with half_bridge vs buck.
   - BLOCKING rules NOT_EVALUATED must block (DESK did not block).
   - Creepage applied as clearance on 1.27 mm-pitch LM5164 and UCC21520 pins must produce a BLOCKING
     engineering decision before routing, never a silent relax and never six failed router attempts.
   - Baseline: FAIL 46/100, 0 tracks, 349 DRC errors (`live-test/ups-1000/reviewer-analysis/FINAL_INDEPENDENT_EVALUATION.md`).
3. **WATCH wearable (ESP32-S3, 4 layers, 35 × 35 mm), requirements-to-BOM traceability.**
   - Source: `projects/WATCH/{REQUIREMENTS.md, BOM_SELECTION.md, POWER_TREE.md, GPIO_MAP.md, LEDGER.jsonl}`.
   - Expected: 20 requirement rows map to the 10 dimensions.
   - The power budget derives 35.4 mA average and about 11.3 h on 500 mAh as a DERIVED record, with
     PROVISIONAL inputs that mark it NEEDS_CONFIRMATION.
   - Every BOM key part has candidates and rejection reasons.
   - The rev B scope change (block 09 removed) appears as a superseding decision.
   - The ledger's "release, all gates passed" (L-0141) followed by DRC 351–442 must be impossible in
     EAK: a release requires fresh gates at that revision.
4. **LIGHT mains LED dropper, the mains safety path.**
   - Source: `projects/LIGHT/{blocks/01_LED_DROPPER.json, LEDGER.jsonl}`.
   - Expected: `mains_safety_isolation` forced; NOT_APPLICABLE on `safety_regulatory` refused.
   - Spacing needs a declared working voltage per HV net, otherwise BLOCKED.
   - A diode package change justified by a 3 mm HV clearance (L-0007) is recorded as a sourced decision.
5. **False-success and partial-mutation matrix.**
   - Source: `tests/test_reliability_api.py`, failure registers F-004/F-005/F-018.
   - Inject engine/bridge faults. Expected:
     - a true return with no change is FALSE_SUCCESS;
     - a delete with survivors is PARTIAL and stays pending;
     - a failed half-built block leaves zero orphan objects after recovery, proven by read-back;
     - a pour that commits no copper is reported FAIL.
6. **Approval binding.** An approval for command hash H at revision N is refused for H′ or N+1; an
   expired or reused approval is refused. This replaces DESK's truthiness check.

## 12. Recommended implementation order (most valuable slices first)

1. **M4.1 requirements record.** Schema in §2.3, IR plus commands plus migration plus completeness and
   gating; ADR. Fixtures: UPS_1000 scoped gate, WATCH derived power budget.
2. **Unified outcome enum and the BLOCKING-NOT_EVALUATED-blocks rule.** Spans `eak-validation`,
   `eak-engineering` and `eak-manufacturing`; ADR.
3. **Rule catalogue as data plus composer plus evidence ladder.** Generate from DESK `rule_sets` like
   `desk_domains.json`, with evidence states preserved. Start with the 39 ENFORCED rules, keep the 25
   ADVISORY as review prompts, and leave the 2 UNSUPPORTED as UNSUPPORTED.
4. **Spacing split plus `TableDataset` provenance plus `EngineeringDecision`.** Fixture: UPS-1000.
5. **Component chain.** Equivalence, the inventory classifier, the evidence-record validator,
   `Candidate` / `DatasheetFact` / `PinMapEvidence`.
6. **Per-command postconditions, bound approvals, and PARTIAL/UNKNOWN kept pending.** Applies to the
   engine and to any future bridge.
7. **Release status ladder.**
8. **One `WorkflowStage` model** replacing the five overlapping ones.

**Work this scout found that should ship:** items 1–3 are concrete and ready to brief. The §2.3 draft is
ready for the M4.1 worker. No EAK bug was reproduced, since this was a read-only study.

## 13. Open questions and caveats
- No DESK or EAK test was run here (brief constraint). Test names are cited from source.
- **Line numbers.** DESK line numbers are from commit 9e44e7c; the readers' `~` line numbers are
  approximate.
- **Label staleness.** EAK maturity labels in `STATUS.md` may be stale (§1).
- **Unconfirmed standard readings.** The IPC-2221B A5/A6/A7 label swap and the mixed PD1/PD2 creepage
  basis (§4.3, item 5) are my reading. They need checking against the standard text by someone who has
  it. That is a verification task, not a product decision.
- **Captain decisions.** No captain-owned decision came out of this study. Every recommendation follows
  existing EAK rules (AGENTS.md non-negotiables) or the execution brief's own specification (§8.1, §8.2,
  §8.7).
