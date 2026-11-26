# ADR-0057: Schema v9: the structured requirement record, its status semantics and the completeness and blocking evaluator
- **Date:** 2026-11-26
- **Status:** Accepted (execution brief goal M4.1, §8.1 and §7.2 B/D; task eak-req-model).
- **Context:** A project needs a structured, editable requirement record before part selection:
  function, supplies and rail tree, interfaces, mechanical envelope, environment, safety and
  isolation, manufacturing, cost and supply, and firmware and lifecycle (brief §8.1). Every value
  needs a unit where one applies, a source, a status, a timestamp and an owner, and a missing value
  must never become zero or a silent default (§7.2 B). EAK also has to show which downstream
  operations a missing value blocks (§7.2 D). Before v9, EAK-IR had `Project::requirements`
  (free text with an id only). The structured spec in `eak-harness` (`design_spec_set`,
  ADR-0020) is a flow record outside the IR, so it is not authoritative project data and does not
  go through `Engine::execute`. DESK's `docs/00_REQUIREMENTS.md` (the founder's taxonomy: ten
  dimensions, "unknown is allowed, blank is not", provisional values with risk, written
  confirmation, a gate before part selection) was read as the domain reference. `Project`,
  `Command` and the harness tools are shared contracts, so the change is recorded here.

## Decision
- **One record in EAK-IR:** `Project::requirement_set: RequirementSet` (module
  `eak_core::requirements`), `#[serde(default)]` and omitted from the file when empty. The
  free-text `requirements` list stays as it is (notes, `AddRequirement`).
  - `fields: BTreeMap<key, ReqEntry>` holds project-level fields keyed by catalogue key
    (`supply.input_voltage`).
  - `items: BTreeMap<id, ReqItem{id, kind: rail | interface, name, parent, fields}>` holds rails
    and external interfaces. Each has its own catalogue fields. Rails form the rail tree through
    `parent`, the rail each one is generated from. Only a rail has a parent, and the tree has no loops.
  - `ReqEntry{status, value, source{kind, reference}, derived_from, owner, risk_if_wrong, note, updated_at_ms, updated_revision}`.
    Source kinds: client, engineer, document, standard, datasheet, measurement, assumption,
    calculation, agent_proposal.
    The engine stamps `updated_at_ms` (wall clock, ms since the Unix epoch) and `updated_revision`
    (the revision the command creates, which the journal ties to its actor). A requirement path is
    the key, or `<item id>/<field>`.
  - `ReqValue` is `number{value, unit, tolerance_pct?}`, `range{min?, nom?, max?, unit}` (at least
    one limit, in order), `text` or `flag`. Units are a closed list serialised as their symbols
    (V, mV, kV, A, mA, uA, W, mW, mAh, m, mm, um, degC, Hz–GHz, bps–Mbps, s, ms, h, day, year,
    percent, g, gn, pcs, KiB, MiB, USD, EUR, GBP, INR, CNY). A unit must measure what the field
    asks for, and only voltage, current and temperature may be negative.
- **Status semantics** (explicit, and checked in `entry_problems` whenever the record is written or loaded):

| Status | Value | Also needs | Level for operations |
|---|---|---|---|
| `UNKNOWN` (also: no entry) | none | (owner and note may say who answers it) | missing |
| `NEEDS_CONFIRMATION` | required | source, the owner who must confirm it, `risk_if_wrong` | provisional |
| `CONFIRMED` | required | source and owner; **only a person** | firm |
| `DERIVED` | required | `derived_from` (existing paths, not itself); **only a person** | firm only if every input is firm and none was recorded at a later revision; else provisional (an UNKNOWN input, a loop or a stale value is reported as an issue) |
| `NOT_APPLICABLE` | none | a reason in the note, a field the catalogue lets be N/A; **only a person** | firm |

  An item list (`supply.rails`, `interfaces.list`) is satisfied by its items. Its own entry can
  only be UNKNOWN, or NOT_APPLICABLE while no items exist. Status is the single confidence
  axis: no second "confidence" number exists that could disagree with it. `risk_if_wrong` is
  carried only by NEEDS_CONFIRMATION.
- **Catalogue:** `FIELDS` (53 project fields in ten sections, which are DESK's ten dimensions as
  the eak-desk-study report §2.3 names them: function, supply, power_budget, interfaces,
  mechanical, environmental, safety_regulatory, manufacturing_test, cost_supply,
  lifecycle_firmware),
  `RAIL_FIELDS` (voltage min/nom/max, peak, continuous and average current, loads) and
  `INTERFACE_FIELDS` (protocol, connector, pinout, signal level, data rate, cable length, ESD).
  Every field has a label, a value kind and measure, whether it may be N/A, whether it is
  *material* (a missing value changes architecture or safety), and one targeted question.
- **Evaluator:** `requirements::evaluate(&RequirementSet) -> RequirementsReport` is pure and
  deterministic. It returns completeness overall and per section (counts by status and level, plus
  percent filled and firm), every field's evaluated state, readiness for nine operations and what
  blocks each one, questions for missing material fields with the operations they block,
  issues, `not_implemented` and `limitations`. The operation table (`OPERATIONS`) is EAK's own
  engineering-order dependency table:

| Operation | Needs (P = provisional or better, F = firm) |
|---|---|
| architecture | P: function, performance, modes, sources, input voltage, rail tree and each rail's voltage, operating temperature, highest working voltage, isolation, quantity |
| part selection | P: function, input voltage, rails (voltage, peak and continuous current), peak and average current, operating temperature, top and bottom height, quantity, assembler, assembly process, BOM target, part availability, interfaces (protocol, connector) |
| schematic capture | P: rail tree (voltage, loads), reverse polarity, interfaces (protocol, signal level, ESD, pinout) |
| power sizing | F: input voltage, rail tree (voltage, peak and continuous current), peak current, operating temperature |
| board outline | F: maximum length and width, thickness, mounting, enclosure |
| placement | F: outline and heights; P: connector positions, interfaces (connector) |
| routing | F: rail tree (continuous current), highest working voltage, isolation; P: interfaces (data rate) |
| safety spacing | F: highest working voltage, isolation, safety standards, sector, altitude, condensation, ingress |
| manufacturing release | F: every field of the record |

- **Commands** (all through `Engine::execute`, one undo step each):
  - `SetRequirementField { path, status, value, source, derived_from, owner, note }` replaces the
    whole entry. Identical content is "nothing to do". Class: reversible local.
  - `ClearRequirementField { path }` makes a field UNKNOWN again. It is refused while a DERIVED
    value names the field. Class: reversible local.
  - `UpsertRequirementItem { item_id?, kind, name, parent? }` creates an item (and returns its id),
    or renames or re-parents one. An item's kind never changes. Items cannot be added while their
    list is NOT_APPLICABLE. Class: reversible local.
  - `DeleteRequirementItem { item_id }` removes an item and its fields. It is refused while a rail
    is generated from the item or a DERIVED value elsewhere names one of its fields. Class:
    destructive.
  - **The AI never self-certifies:** `Engine::execute` refuses (precondition) a
    `SetRequirementField` with CONFIRMED, DERIVED or NOT_APPLICABLE from an agent actor,
    including inside a batch. An agent records NEEDS_CONFIRMATION (source kind `agent_proposal`
    or `calculation`) for a person to confirm.
  - The agent tool schemas cover all four commands (`eak-intelligence` tool count 93 → 97). The
    harness counts them as requirement changes for task invalidation.
- **Read access:** the read tool `get_requirements` (`eak-intelligence` read tools, so it is in
  the harness, MCP and `eak call`), plus the CLI shortcut `eak requirements`. It returns the
  record, the evaluation and the catalogue (paths, units, operation table).
- **Integrity:** `RequirementSet::problems` runs in `eak_core::integrity::check`, which the engine
  runs after every apply (so an invalid record is refused and nothing changes) and which runs when a
  file loads. It checks catalogue keys, status rules, item ids and names, rail parents and loops,
  and `derived_from` references.
- **Migration v8 → v9:** only the version number changes. Absent means empty (every field
  UNKNOWN, no rails or interfaces), which is exactly the v8 meaning. A v9 file is refused by v8
  builds, which would otherwise drop the record silently.

## Alignment with the DESK study (eak-desk-study report §2, schema proposal §2.3)
Adopted:
- One requirements store in EAK-IR, changed only through typed commands.
- The five statuses and their meaning.
- DESK's ten dimensions as the sections.
- NEEDS_CONFIRMATION carries `risk_if_wrong` and the `owner` who must answer it (DESK
  PROVISIONAL, risk noted).
- CONFIRMED and NOT_APPLICABLE are human only, and NOT_APPLICABLE carries a rationale.
- An agent may not record DERIVED.
- A DERIVED value goes stale when an input is recorded at a later revision.
- Status is per value, not per row: each field and item field has its own entry, which removes
  the need for `parts`.
- Gate state is computed by `evaluate` and never stored.
- Compliance (PASS/FAIL) stays off the record.
- `updated_at` and the revision are stamped by the engine.
- The free-text requirements are kept verbatim and nothing is auto-confirmed.

Diverged, with the reason:
- **No `R-NN` ids and no supersede-instead-of-delete.** The catalogue key (or
  `<item id>/<field>`) is the stable id, and the operation journal and undo history keep every
  earlier entry. A second id would have to be kept in step with the key.
- **No `quantity` key separate from the path.** The path is already the machine key that
  checks bind to.
- **No `statement` field.** The catalogue's label and question say what the field means. The
  client's words go in the source reference or the note.
- **No `limit_kind`.** A range value carries its min, nom and max, and the catalogue field
  defines what it limits.
- **No `priority`.** The catalogue's `material` flag says which missing values change
  architecture or safety and get a question. Must, Should and May would be a second,
  unchecked axis.
- **No `confirmed_by` or `confirmed_at`.** For CONFIRMED, `owner` is the person who confirmed it,
  and the engine-stamped `updated_at_ms` and `updated_revision` are when; the journal names the
  actor of that revision.
- **No `derived_from{calc_id, evaluator, evaluator_version}` or `inputs@revision`.** The input
  paths are stored, and staleness compares the inputs' `updated_revision` with the derived
  entry's.
- **Persons may record DERIVED.** DESK wants DERIVED written only by a deterministic evaluator.
  No such evaluator exists yet, so a person records it with its inputs, and the evaluator is
  listed in `not_implemented`.
- **Not built yet, and listed in `not_implemented`:**
  - `condition` and `scope`, with scoped gates (UPS_1000 "PASSED for the low-voltage prototype
    only");
  - domain-required fields from the classification (a NOT_APPLICABLE on safety refused under a
    mains or HV class), which needs the M4.3 classifier;
  - `affects` traceability;
  - folding `design-flow/spec.json` into this record. That changes the harness design flow
    (ADR-0020) and is a follow-up of its own.
- **No source `locator` or `edition`, and the source kinds are mapped.** These go in the
  `reference` text. DESK's ClientDocument and Person are `client`, `document` and `engineer`.
  BuilderDecision is `engineer` or `assumption`. Calculation and AgentProposal were added as
  `calculation` and `agent_proposal`.
- **Free-text `Requirement{id, text}` is not converted.** It has no catalogue key to bind to, so
  it stays as notes. This also guarantees the migration keeps every text verbatim and confirms
  nothing.

## Options considered
- *Promote the harness `DesignSpec` into the IR.* Rejected. It requires numbers it cannot have
  yet (`i_max_a > 0`, ambient min/max as plain numbers), so an unknown value cannot be stored
  honestly. It also has no status, source or owner per value.
- *A free-form key/value map.* Rejected. Without a catalogue there is no completeness, no units
  check and no blocking table.
- *Typed Rust fields per requirement.* Rejected. With over 60 fields, every new field would mean a
  schema change. The catalogue keeps the stored shape stable (`fields` map plus `items`) while the
  set of fields stays reviewed and versioned in code.

## Consequences
- M4.2 (intake and readiness UI) and M4.6 (design profile from confirmed or derived values only)
  build on this record and on `evaluate`. No UI is part of this record.
- Not implemented (listed in every report): cross-field consistency (rail currents against the
  budget, rail voltages against the input range); enforcement (no engine refuses an operation on
  this report yet, so readiness is advisory); requirement-to-constraint compilation and
  traceability.
- Limitations: completeness counts presence and status, so a CONFIRMED value is only as right as
  its source. The operation table is EAK's own, not a standard. One item satisfies an item list,
  and whether the list is complete is the owner's call. The free-text `requirements` notes are
  not part of completeness.
- Source requirement: execution brief §8.1, §7.2 B and D, M4.1. Spec §2.1 (one canonical EAK-IR,
  typed commands only).
