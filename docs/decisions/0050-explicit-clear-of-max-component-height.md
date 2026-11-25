# ADR-0050: An explicit clear for the max component height in `SetDesignRules`
- **Date:** 2026-11-25
- **Context:** Phase 1 definition-of-done audit, defect S19 (fix slice F10). `SetDesignRules`
  treats every `None` field as "leave unchanged". That is right for the required board rules, but
  `Constraints.max_component_height` is itself optional in the IR (`Option<Nm>`; no limit means
  MECH-001 is not evaluated and the release gate says so). The Design rules dialog sent `null` for
  an emptied "Max component height" field, so the engine kept the old limit (or refused the
  command as "no change"): once set, the limit could never be removed from the UI, CLI or AI.
- **Options considered:**
  1. Read `max_component_height: null` as "clear". Rejected: it changes the meaning of every
     existing caller that sends the full field set with `null` for "unchanged" (the dialog itself,
     the AI tool schema "omitted fields are unchanged", the manufacturing tests), so an unrelated
     rules edit would silently drop the limit.
  2. A separate `ClearMaxComponentHeight` command. Rejected: one more command and tool for one
     field, and a dialog Apply that edits several rules and removes the limit would need a `Batch`.
  3. **Chosen:** an additive flag `clear_max_component_height: bool` on `SetDesignRules`,
     `#[serde(default)]` and omitted from JSON when false.
- **Decision:** with the flag set the engine sets `max_component_height` to `None`. Giving the
  flag together with a `max_component_height` value is refused (`Invalid`), so a command never
  means both. The Design rules dialog sends the flag only when the field is emptied while a limit is
  stored; an empty field with no stored limit stays a no-op. The field label says "empty = no
  limit". The AI tool schema lists the flag.
- **Consequences:** a shared-contract (`Command`) change with no IR or schema-version change and
  no project migration: command JSON written before this ADR (journals, scripts, agent calls) has
  no flag and decodes as "do not clear", and a command without the clear serialises exactly as
  before. The clear is one revision and one undo step like any rules edit. Tests:
  `crates/eak-runtime/tests/design_rules.rs` (clear, refusal of value plus clear, undo, JSON round
  trip and older JSON), vitest `rules.test.tsx` ("S19 regression"), ui-evidence
  `scenarios/f10-rules-reasons.json` `s19-*`.
- **Source requirement:** Product spec, verification honesty (a constraint the user removed must
  not keep driving MECH checks); execution instructions, "every design mutation goes through a
  typed command".
