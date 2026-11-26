# ADR-0060: Schema v11: conflicting requirement values, and the domain classification as DERIVED evaluator output
- **Date:** 2026-11-27
- **Status:** Accepted (execution brief goals M4.2 and M4.3, §8.1, §8.2 and §7.2 A-C; task
  eak-domain). Builds on ADR-0057.
- **Numbering:** schema v10 and ADR-0059 are #155 (reference-plane limits, merged), and ADR-0058
  is held by open PR #152, so this change takes v11 and ADR-0060.
- **Context:** ADR-0057 recorded requirement values, but `SetRequirementField` replaced the whole
  entry, so a second source with a different value silently overwrote the first. The brief
  (§8.1, M4.2) asks for conflicting values to be shown as conflicts and resolved by a person. M4.3
  asks for a pure, deterministic domain classifier with explicit evidence for every class,
  NEEDS_CONFIRMATION where the evidence is weak, and output stored as DERIVED with evaluator
  provenance. The DESK study (report §2 and §3; `docs/research/DESK_TO_EAK_DOMAIN_WORKFLOW.md`)
  gives the taxonomy and lists the DESK classifier's defects. `ReqEntry`, `Command` and the harness
  tools are shared contracts, so the change is recorded here.

## Decision
- **Conflicts in the IR (v11).** `ReqEntry.conflicts: Vec<ReqEntry>` holds competing values not
  yet resolved. It is `#[serde(default)]` and omitted when empty, so a v9 or v10 file has none, which is
  exactly the v11 meaning. Integrity rules:
  - only an entry with a value has conflicts;
  - each candidate has a value that differs from the recorded value and from every other candidate;
  - candidates have no nested conflicts;
  - each candidate passes the status rules of ADR-0057 itself.
- **Recording never overwrites by default.** `SetRequirementField` gains `supersede: bool`
  (default false), and the engine calls `RequirementSet::record`.
  - With no entry, or with the same value, the entry is written (the same value keeps its open
    conflicts).
  - A different value is added as a conflict candidate and the response warns. An identical
    candidate is a no-op.
  - Clearing a value through a set (no value over a recorded one) is refused; use
    `ClearRequirementField`.
  - With `supersede: true`, the new entry replaces the old one and its conflicts.
- **A conflicted value is missing.** While an entry has conflicts, the evaluator gives it level
  Missing, puts "Resolve the n conflicting value(s)" first in its question, and counts it in
  `Completeness.conflicts`. `FieldReport.conflicts` lists the candidates.
- **`ResolveRequirementConflict{path, keep, owner, note}`** keeps the recorded value (`keep` 0) or
  candidate n. The kept value becomes CONFIRMED by `owner`: resolving is a person's decision.
  `derived_from` and `risk_if_wrong` are cleared, and the other values are dropped (the journal
  and undo keep them). It is one undo step.
- **Agents, checked against the project state before the command (including inside batches):**
  - An agent cannot record CONFIRMED or NOT_APPLICABLE (as in ADR-0057), nor resolve a conflict.
  - An agent cannot replace (supersede), restate or clear a value a person decided (CONFIRMED or
    NOT_APPLICABLE). It can add a different value beside one, as a conflict for a person to resolve.
  - An agent cannot supersede or clear an entry that has conflicts.
  - `engine::human_only` now takes the project.
- **Domain classes are requirement fields.** `domain.<id>` flag fields are listed in
  `DOMAIN_CLASSES`: archetypes first, then overlays. Each class records its DESK origin and the
  risk if it is wrong.
  - The classes are a new catalogue section, `Section::Domain`, which stays outside completeness.
    Once a class is recorded, it gates architecture and part selection (Provisional) and
    manufacturing release (Firm).
  - A recorded safety class (ac_mains_hv, high_voltage, galvanic_isolation) gates safety spacing
    (Firm).
  - `true` means the class applies. A person's CONFIRMED `false` is a rejection.
  - Clearing or deleting a requirement that a classification names is never blocked by the
    classification.
- **The classifier:** `eak_engineering::domain::classify` is pure and deterministic. For each class
  it lists the requirement values that support it and those that contradict it, each with the rule
  it meets, its strength and the level of the value.
  - **Strong evidence** is a structured value, or text in the field whose purpose is that property
    (an interface protocol, a power source, an isolation requirement). **Weak evidence** is
    keywords in the function description, or a value whose presence only hints at the class.
  - With no supporting evidence, the class is **not indicated**, and the deciding inputs that are
    still UNKNOWN are named. Not indicated does not mean ruled out.
  - Evidence both for and against a class is a **Conflict**. Strong evidence from a firm value
    with nothing against is **Applies**. Anything else is **NeedsConfirmation**. There is no score
    or confidence number.
  - `not_implemented` and `limitations` are part of the result.
- **`ClassifyDomain`** (EngineeringImpacting; a person or an agent may run it) records the result
  through `domain::plan`:
  - Applies is recorded DERIVED `true`, with `derived_from` set to the supporting paths, the
    evidence in the note, and source `calculation` naming the evaluator and its version.
  - NeedsConfirmation is recorded NEEDS_CONFIRMATION `true`, with the class's risk. The
    classifier's proposal needs no owner: a person confirms it.
  - Conflict is recorded NEEDS_CONFIRMATION `true`, with a candidate `false` that carries the
    evidence against.
  - An entry anyone else recorded is never replaced. When the classifier disagrees with it, the
    classifier's view is added as a candidate, and it is removed again once the classifier agrees.
  - Classifier entries without evidence are removed. A value someone else recorded against a
    classifier entry is never dropped by a later run: it stays as a conflict, becomes the entry
    when the classifier now agrees with it, or takes the entry's place when the evidence goes.
  - When nothing changes, the command is refused (nothing to classify).
  - A DERIVED class goes stale when an input changes later (ADR-0057 staleness).
  - `SetRequirementField` refuses DERIVED for everyone: only ClassifyDomain writes it.
- **DESK defects fixed** (study §3.2):
  - no overlay is inherited from an archetype's "typical overlays" (the motor_drive false positive);
  - an 18–36 V input no longer implies industrial interfaces;
  - `mains_safety_isolation` is split into `ac_mains_hv`, `high_voltage` and `galvanic_isolation`;
  - there is no confidence number;
  - keyword evidence alone never yields DERIVED;
  - mains involvement with contradicting evidence is a conflict, never settled silently.
  - The rule-engine defects (NOT_EVALUATED blocking, PASS semantics, strictness keys) belong to
    the rule registry (M4.4).
- **Interfaces:**
  - Tools `ResolveRequirementConflict` and `ClassifyDomain` (99 command tools), plus `supersede`
    on `SetRequirementField`.
  - `get_requirements` adds the live `classification`, `recorded_up_to_date` and
    `catalogue.domain_classes`.
  - CLI `eak requirement set <REV> <JSON>` and `eak requirement classify <REV>`.
  - The desktop Requirements panel reads `requirements_report` over IPC, and every edit goes
    through `Engine::execute`.

## Options considered
1. **Last write wins with history in the journal** (the v9 behaviour): a disputed value looks
   settled. Rejected: the brief requires conflicts to be visible.
2. **A separate conflict list beside the record:** a second representation that can drift from the
   entries. Rejected.
3. **Candidates on the entry** (chosen): one record, the value in force stays visible, and the
   level rules see the dispute.
4. **Domain classes as a separate IR type:** another representation with its own status semantics.
   Rejected for flag fields that reuse the ADR-0057 status, source, staleness and owner rules.
5. **A scored classifier (DESK):** a number hides weak evidence and contradictions. Rejected.

## Consequences
- v11 files are refused by v10 and earlier builds. Those builds would drop the conflicts silently and show a
  disputed value as settled.
- Keyword evidence matches English whole words only. Power conversion has only indirect
  evidence in the record, so a person confirms it.
- Not built yet:
  - DESK's sub-archetypes and remaining archetypes;
  - domain-required fields;
  - rules per class (M4.4);
  - BOM (M5).
- **Source requirement:** Execution brief §8.1, §8.2, §7.2 A-C (M4.2, M4.3); DESK study §2–§3.
