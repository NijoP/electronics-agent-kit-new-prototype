# Reworking an existing EAK project (agent guide)

Use this when a project already exists and needs corrections. Do not rebuild it. Tools:
`rework_*` (MCP) or `eak rework ...`. Design: ADR-0022. Code-reviewed only, never run.

## Loop
1. `rework_inspect` (read-only). Pass `expect_project_id` / `expect_name` to confirm you are in the
   right project. Read the check totals and unrouted connections.
2. `rework_baseline {confirm_project_id}`: immutable snapshot + fingerprints; returns `session_id`.
   Never repeat it to "reset": a new session is a new baseline of the *current* state.
3. `rework_audit`: issues with `issue_id`, `category`, `severity`, `affected {ids, nets}`.
   Categories run in dependency order: schematic, placement, power_thermal, routing, copper, drc.
4. Optional `rework_protect {ids, reason}` for anything the human said must not change. Locked
   footprints are always protected. Protection cannot be removed by a tool.
5. `rework_plan {title, repairs:[{issue_id, description, steps:[{type:"MoveFootprint",...}]}]}`.
   One repair per issue; steps are typed commands. Refused (nothing stored) when a step changes a
   protected or out-of-scope object, or uses `AutoPlace`, `ClearRouting` without `net`, or
   `AutoRoute` without `nets`. Repairs are re-ordered by dependency.
6. **A human approves**: `eak rework approve <PLAN_ID> <FINGERPRINT>` in an interactive terminal.
   Ask the user to run it; you cannot (MCP / pipes are refused).
7. `rework_apply {plan_id, expected_revision}` (the plan's base revision). Each repair is one
   revision. After each, EAK reads the revision back and re-hashes every protected and accepted
   object; a violation undoes that repair and stops (`integrity` error with the object ids).
   ERC/DRC/MECH/DFM run afterwards.
8. `rework_report`: diff vs baseline, issues resolved/remaining/new, revisions made outside
   rework, `not_implemented`, `limitations`, label. Labels: `integrity_violation`,
   `changes_outside_rework`, `repairs_incomplete`, `not_audited`, `passed_deterministic_checks`.

## Rules
* If the revision moved after you planned (`stale_revision`), re-audit and re-plan.
* Report the label and the `not_implemented` list as they are. Never write "manufacturing-ready".
* Changes made outside this flow show up as `unattributed_revisions`; tell the user.
* Re-routing: after `AutoRoute` repairs run `verify_routing` (connectivity + DRC).
