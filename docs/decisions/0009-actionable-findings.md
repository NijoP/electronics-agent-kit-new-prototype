# ADR-0009: Actionable ERC/DRC/MECH findings
- **Status:** Accepted. Agreed on 2026-11-13 (~22:33 IST) by the main firstmate, acting as integration
  authority for the platform-core (`eak-core`) and desktop (`eak-desktop`) owners. Agreed fields,
  exactly as proposed: `id` `"<rule_id>-<12 hex>"`; `measured` `Quantity { value, unit }`;
  `required` `Limit { bound: min|max, value, unit }` with string units; `suggested_fix`
  `Option<String>`. All new fields are `#[serde(default)]`, so old JSON loads with an empty id. No new
  dependency (`sha2` was already present).
- **Date:** 2026-11-13
- **Context:** The native schematic and PCB editors and the AI assistant need findings they can
  act on: which objects and where, which rule, the measured value against the required one, and
  what to change (final directive §2 "ERC and actionable error reporting", §3 "DRC feedback with
  actionable corrections", §11.6). `Finding` is a shared report type, so its change is recorded here.
- **Options considered:**
  1. Leave everything in the free-text `message` (consumers would have to parse prose).
  2. Add a rule-specific payload per rule (a large type surface nobody consumes yet).
  3. Add a few general, optional fields to `Finding`.
- **Decision:** Option 3. `Finding` gains `id`, `measured: Option<Quantity>`,
  `required: Option<Limit>` and `suggested_fix: Option<String>`, all `#[serde(default)]`.
  - `id` is `<rule_id>-<12 hex>` of SHA-256 over the rule id, the sorted object ids and a short
    semantic key (pin number, `drill`/`ring`, track end, net). It does not include measured values,
    so it stays the same while the same violation persists. `Report::new` keeps ids unique with a
    deterministic suffix.
  - `measured`/`required` are set only for quantitative rules (track width, current capacity,
    clearance, edge clearance, drill, annular ring, capacitor voltage rating, component height),
    in `mm` or `V`. `Limit.bound` is `min` or `max`.
  - `location` stays in the editor's own space: schematic for ERC, board for DRC/MECH.
- **Consequences:** Old report JSON still deserialises (empty id, null new fields). The
  `finding(..)` helper keeps its signature, so rules added elsewhere get ids automatically; their
  own measured values and fixes still have to be added. Desktop `types.ts` must add the new fields
  to show them. A suggested fix is a suggestion, never a claim that the design becomes correct.
- **Source:** FINAL_DIRECTIVE_2026-11-14 §2, §3, §11.6; Execution Instructions §7.1 (verification
  findings), §9.1 (engineering honesty).
