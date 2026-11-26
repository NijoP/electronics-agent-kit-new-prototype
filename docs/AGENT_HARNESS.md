# EAK agent harness — guide for coding agents

You are working on an **EAK electronics project** (schematic + PCB in one canonical EAK-IR).
Never edit `project.eak.json` or any project file yourself. Every change goes through the EAK
harness, which submits typed commands to EAK's engine with a revision check, journals them under
your agent name and session, and lets EAK's deterministic validators (not you) judge the result.

## Interfaces (same tools, same JSON)

| Interface | How |
|---|---|
| CLI | `eak <command> --json` (see `eak --help`) |
| MCP (stdio) | `eak mcp` or `eak-mcp` — e.g. `claude mcp add eak -- eak mcp` |
| Any tool by name | `eak call <TOOL> '<ARGS_JSON>' --json` |

List tools with `eak tools --json` (or MCP `tools/list`). Full reference: `docs/ENGINEERING_API.md`.

## Project context

Inside an EAK terminal session these are exported; flags and tool arguments override them:

| Variable | Meaning |
|---|---|
| `EAK_PROJECT_DIR` | absolute project folder (`--project`, arg `project_dir`) |
| `EAK_PROJECT_ID` | the call is refused (`project_mismatch`) if the folder holds another project (`--project-id`, arg `project_id`) |
| `EAK_SESSION_ID` | your session id, recorded in the journal (`--session`) |
| `EAK_AGENT_NAME` | your agent name, recorded in the journal (`--agent`) |
| `EAK_HARNESS_SOCKET`, `EAK_HARNESS_TOKEN_FILE` | the desktop app's harness endpoint (informational; the CLI finds it from `<project>/.harness.json`) |
| `EAK_HARNESS_ALLOW_DESTRUCTIVE=1` | user opt-in for destructive commands (`--allow-destructive`) |
| `EAK_HARNESS_APPLY_MODE=propose` | propose mode (`--propose`, ADR-0051): design tools build a proposal the user reviews and applies in EAK; the project does not change |

There is no global "active project": every call names its project, and every response echoes
`project.id` and `project.revision` — check them.

If the EAK desktop app has the project open, calls go through the app's engine (`route: "app"`)
and the editor shows your edits; otherwise the CLI opens the project directly under its lock
(`route: "direct"`). Two engines never write the same project.

## Live synchronisation (terminal → editor)

The path of one agent edit, from the shell in an EAK terminal to the pixels:

1. **Context.** The terminal session exports `EAK_PROJECT_DIR`, `EAK_PROJECT_ID`, `EAK_SESSION_ID`,
   `EAK_HARNESS_SOCKET` and `EAK_HARNESS_TOKEN_FILE` (`eak-terminal` base env + the space's
   project binding, `apps/desktop/src-tauri/src/lib.rs::terminal_state`). `eak` / `eak mcp` read
   them into a `CallContext` (`crates/eak-harness/src/context.rs`).
2. **Route.** `route::invoke` finds the app via `<project>/.harness.json` and sends one JSON line
   to the endpoint (`endpoint.rs`, token-authenticated). The app handler is
   `apps/desktop/src-tauri/src/lib.rs::harness_call`, which only serves the project the app has
   open (`project_mismatch` otherwise).
3. **Commit.** `eak_harness::handle` runs the tool on the app's engine: a design tool is a typed
   `Command` through `Engine::execute` with `expected_revision` (stale → `stale_revision`) and the
   agent actor `agent:<name>@<EAK_SESSION_ID>`. Nothing writes project files directly.
4. **Event.** If the revision moved, `harness_call` emits `eak://revision-changed`
   (`apps/desktop/src-tauri/src/sync.rs`, ADR-0031): project id and dir, `revision_before/after`,
   `source: "harness"`, the caller's `session_id`, and `areas` — the top-level IR sections
   (`schematic`, `pcb`, `library`, …) read from the engine's journalled change list. Reads and
   rejected commands emit nothing.
5. **Refetch.** The open workspace (`apps/desktop/src/Workspace.tsx`, decisions in
   `revisionSync.ts`) drops events for another project or for a revision it already shows, fetches
   one `current_project` snapshot, and applies it only if it is strictly newer than what is on
   screen. The schematic, PCB and 3D views render from that snapshot; the airwires and
   classification refetch only when an area they read changed. The 1.5 s event poll remains as a
   fallback if an event is missed.

Switching between the Terminal and EDA workspaces keeps both mounted, so terminal sessions and
design state survive. Commits made by the UI itself do not emit the event (the caller already has
the response). Tests: `apps/desktop/src-tauri/tests/live_sync.rs`, `apps/desktop/src/liveSync.test.tsx`
(run in CI; not run locally).

## Workflow

1. `eak info --json` → note `project.id` and `project.revision`.
2. Read: `list_symbols`, `get_component`, `get_netlist`, `list_footprints`, `get_board`,
   `get_connectivity`, `list_library_components`, `get_history`.
3. Modify with **one typed command per call**, passing the revision you read:
   `eak exec <REV> '{"type":"PlaceSymbol","component_id":"R_0805","position":{"x":10000000,"y":20000000}}' --json`
   The response has `revision_after`; use it as the next `expected_revision`. Group related steps
   with the `Batch` command (one revision, one undo step).
4. Verify: `eak check all --json` → structured findings (`rule_id`, `severity`, `message`,
   `location`, `objects`, `suggested_fix`) plus `not_implemented` and `limitations`.
5. Fix findings with further commands, re-check, then `eak export --json` if asked.
6. Report what you changed (revisions, op ids) and the validation results — never claim a check
   passed that you did not run, and never call a design "manufacturing-ready".

Lengths are **integer nanometres** (1 mm = 1 000 000). Rotations are `R0|R90|R180|R270`.

## Operation categories

| Category | Effect | Tools |
|---|---|---|
| `read` | none | `project_info`, `list_symbols`, `get_component`, `list_footprints`, `get_board`, `get_connectivity`, `get_history`, `get_design_summary`, `get_requirements`, `get_netlist`, `list_library_components`, `classify_domain`, `calc_*` (incl. `calc_copper_trace_width`, `calc_copper_voltage_drop`: every input explicit), `power_analysis` |
| `design` | one new revision (needs `expected_revision`) | one tool per EAK command (`PlaceSymbol`, `ConnectPins`, `SyncSchematicToPcb`, `SetBoardOutline`, `MoveFootprint`, `AddTrack`, `AddVia`, `AddZone`, `AutoRoute`, `StitchZones`, `Batch`, ...), `Undo`, `Redo` |
| `simulation` | none (ngspice on a snapshot) | `simulation_status`, `run_simulation` |
| `validation` | stores evidence only | `run_check` (`erc`, `drc`, `mech`, `dfm`, `all`), `run_gates` (fail-closed gates, ADR-0021; only PASS passes) |
| `manufacturing` | writes `output/rev-N/` only | `export_manufacturing` |
| `flow` | spec/plan/claim/report records in `design-flow/`; `design_apply` = one new revision | `design_spec_set`, `design_spec_get`, `design_plan`, `design_apply`, `design_validate`, `design_claim`, `design_iteration_log`, `design_report` (ADR-0020; loop in `docs/agents/AUTONOMOUS_DESIGN.md`) |
| `rework` | evidence in `rework/<session>/`; `rework_apply` = one revision per repair after human approval (`eak rework approve`) | `rework_inspect`, `rework_baseline`, `rework_audit`, `rework_protect`, `rework_plan`, `rework_apply`, `rework_report` (ADR-0022; loop in `docs/agents/REWORK.md`) |
| `knowledge` | none for the design; `knowledge_import` writes only `knowledge/index.json` | `knowledge_search`, `knowledge_lookup`, `knowledge_cite`, `knowledge_import` (ADR-0024): advisory text with source/page/revision/hash, **never a validation result** |
| `task` | `<project>/agent-tasks.json` only (progress record, never evidence) | `agent_task_begin`, `agent_task_finish`, `agent_task_status` (ADR-0036): phases are recorded from observed calls; `completed` is granted only when evidence supports it. `SyncSchematicToPcb` is refused (`precondition`, `details.gate = "pcb_sync"`) until ERC has run on the committed schematic |
| `proposal` | `<project>/proposals/` only; never applies | `proposal_begin`, `proposal_status`, `proposal_list`, `proposal_submit`, `proposal_discard` (ADR-0051, see "Propose mode") |
| `weft` | `<project>/weft/journal.jsonl` only; never the design | `weft_register`, `weft_agents`, `weft_send`, `weft_inbox`, `weft_tasks`, `weft_conflicts`, `weft_history` (ADR-0053, see "WEFT orchestration") |
| `project` / `discovery` | creates a folder / none | `create_project`, `list_tools`, `guide`, `design_spec_schema` |

## Requirements (schema v9, ADR-0057)

`eak requirements --json` (tool `get_requirements`) returns the structured requirement record,
its evaluation and the field catalogue: completeness per section, which operations (architecture,
part selection, schematic capture, power sizing, board outline, placement, routing, safety
spacing, manufacturing release) each missing or unconfirmed value blocks, and targeted questions.
A field with nothing recorded is `UNKNOWN`. Never put a number where nobody gave one.

- Record a value: `{"type":"SetRequirementField","path":"supply.input_voltage","status":"NEEDS_CONFIRMATION","value":{"kind":"range","min":4.5,"nom":5.0,"max":5.5,"unit":"V"},"source":{"kind":"client","reference":"kick-off call 2026-11-20"}}`.
  A rail or interface field is `<item id>/<field>`. Add the item first with
  `UpsertRequirementItem` (`kind` `rail` or `interface`; a rail's `parent` is the rail it is
  generated from).
- As an agent you record `UNKNOWN`, `NEEDS_CONFIRMATION` or `DERIVED` (with `derived_from`). Only a
  person records `CONFIRMED` or `NOT_APPLICABLE`: the engine refuses them from an agent
  (`precondition`). Ask the person, and quote the report's question for the field.
- Readiness is advisory: no tool refuses an operation on it yet. Say which operations are
  still blocked when you report.

## Board flow (DESK method; heuristics, not a certification)

1. Mark where each supply enters the board: library connector pins are passive, so put a power
   port (the PWR_FLAG equivalent) on the connector pin of every external rail and of GND:
   `{"type":"LabelPin","symbol_id":"<J1>","pin":"1","name":"VBUS","kind":"power"}`. Otherwise
   ERC-003 reports "nothing drives it"; its fix text names the exact command.
2. `SyncSchematicToPcb`, `SetBoardOutline`, `AutoPlace`, then `AutoRoute` (signals and power).
   If no connection can be routed, AutoRoute fails with `precondition` and lists each connection
   and why; `nothing_to_do` names the nets still unrouted.
3. Pour GND on **every** copper layer (`AddZone` per layer over the board), then
   `StitchZones {"net":"GND"}`: escape via + short track for each GND SMD pad the pour of its
   own layer does not reach, and a stitching via for each pour island without one. Islands that
   touch no GND copper are dropped from the fill as dead copper.
4. `run_check all`, then `run_gates`; fix what they report and repeat. Nothing here certifies
   the board.

## Errors

Every response is `{"ok", "tool", "category", "route", "project", "session", "result" | "error"}`.
`error.code` is one of: `unknown_tool`, `invalid_arguments`, `no_project`, `project_not_found`,
`project_exists`, `project_mismatch`, `project_locked`, `missing_expected_revision`,
`stale_revision` (re-read, re-plan, retry), `approval_required` (destructive command without the
user's opt-in), `not_found`, `precondition`, `integrity`, `storage`, `nothing_to_do`,
`simulator_unavailable`, `simulation_failed`, `export_failed`, `app_unreachable`, `auth_failed`,
`internal`. CLI exit codes: 0 ok, 1 error, 2 usage, 3 validation errors found, 4 stale revision.

## Propose mode (ADR-0051)

When the user starts your session in propose mode (`--propose` or
`EAK_HARNESS_APPLY_MODE=propose`), nothing you do changes the project or its revision:

- A design tool call is checked on a copy of the project (same command code and integrity check
  as the engine) and added as a step to your session's **open proposal**, started at the current
  revision. The response has `"proposed": true`, `proposal_id`, the `created` ids and the
  `changes`; `revision_after` equals `revision_before`. Pass the same `expected_revision` (the
  project's revision) on every call. Objects you create keep the ids you were shown, so later
  steps may name them.
- Reads, `run_check`, `run_gates` and simulation show the copy with your open proposal applied
  (`result.proposal_view`); such checks store no evidence. `project_info`, `get_history` and
  `intel_*` show the project itself.
- `Undo`, `Redo` and `rework_apply` are refused (`precondition`); `design_apply` adds its plan's
  steps to the proposal.
- `proposal_begin {title, rationale}` names it (the rationale is shown to the reviewer as
  *assumed*); `proposal_status` gives EAK's review: each step's changes and the steps it
  `requires`, the total delta, ERC/DRC before and after on the copy (findings that appear or are
  resolved, rules newly UNKNOWN, labels), and `problem` if it cannot be applied;
  `proposal_submit` marks it ready; `proposal_discard` drops it.
- **There is no apply tool.** The user applies the proposal in EAK: Apply is refused unless the
  project is still at the proposal's base revision, saves a checkpoint
  (`checkpoints/rNNNNNN-before-<id>`), and executes the chosen steps as one `Batch` — one
  revision, one undo step, journalled as your agent with the user's approval. If the project moved
  on, the proposal is stale: discard it and propose again at the new revision.


## WEFT orchestration (ADR-0053)

Agents on one project coordinate through typed messages in `<project>/weft/journal.jsonl`. The
registry, the tasks and every inbox are rebuilt from that journal, so the history is the state.

- **Register.** `weft_register {tool, model, capabilities}` registers the session as
  `agent:<name>@<session>`, the label the engine journal uses. `weft_agents` lists who is there.
- **Delegate.** `weft_send {kind: delegate, to, title, depends_on?}` creates a task owned by `to`.
  - Only the owner sets its status.
  - `in_progress` and `done` are refused until every `depends_on` task is `done`.
  - `weft_tasks` shows what each task waits on and whether it is ready.
- **Talk.** `weft_send` also takes `context_share`, `finding`, `status` and `blocker`.
  `weft_inbox {since}` returns what was sent to you or to everyone; pass the previous
  `next_since`.
- **Review.** `weft_send {kind: review_request, proposal_id, to: <agent>|engineer}` routes a
  proposal to a reviewer other than its author. That reviewer answers with
  `{kind: review_verdict, proposal_id, verdict}`. A verdict is advice: only the engineer applies a
  proposal, in EAK.
- **Conflicts.** `weft_conflicts` lists open proposals that are `stale` (made at another revision)
  or that `overlap` (touch the same object). Applying one makes the others stale, and a stale
  proposal is refused, so nothing is overwritten silently. Reject one, or propose again on the
  new revision; EAK does not rebase.
- **History.** `weft_history` returns every event with its agent, task, proposal and revision.
- **Launched sessions.** A launcher (Settings › AI tools) that starts `eak-mcp` with
  `EAK_AGENT_TOOL` (plus `EAK_AGENT_MODEL`, `EAK_AGENT_CAPABILITIES`, `EAK_AGENT_NAME`,
  `EAK_SESSION_ID`) gets the session registered automatically once the project exists. Other
  sessions call `weft_register`.
