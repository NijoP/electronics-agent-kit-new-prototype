# ADR-0051: Propose mode for harness sessions, and proposals applied by a person
- **Date:** 2026-11-26
- **Context:** Slice S8 of the Traycer review (`report.md` §5 B-2/B-3/B-6, §10, §11.3 "Work out:
  proposals, not edits"). A harness session today executes every design tool at once under a
  standing approval (ADR-0018, `tools.rs::design`). That suits a developer at a terminal, but the
  co-pilot (S10) has to show an engineer what an AI wants to change before anything changes. The
  pieces already existed but were not joined up. The ECO dialog computes a sync on a copy and
  applies it as one `Batch` at the previewed revision (ADR-0041). `ai_propose` dry-runs steps on a
  sandbox but keeps them in process memory and applies them one undo step each (`ai.rs`). The
  engine has checkpoints that nothing calls. The proposal has to be one engine that the co-pilot,
  terminal agents and the review dialog (S9) all share, so it lives in `eak-harness` and has no UI
  in it.
- **Options considered:**
  1. Run agent edits on a long-lived sandbox `Engine` per session and copy its journal over on
     Apply. Rejected: copying the journal is a second write path, and a sandbox held in memory
     does not survive the separate `eak` processes of a terminal agent.
  2. Store the proposal as the `Change` delta computed on a copy, and apply that delta. Rejected:
     applying a delta bypasses `Engine::execute`, the command preconditions and the policy, which
     breaks the AGENTS.md rule that every design mutation is a typed `Command`.
  3. **Chosen:** store the proposal as typed commands (its steps), recompute everything from the
     project on every read, and apply the chosen steps as one `Batch` through `Engine::execute`.
- **Decision:**
  - `CallContext.apply_mode: apply | propose` (serde default `apply`, so older endpoint clients
    parse unchanged), set by `EAK_HARNESS_APPLY_MODE=propose` or `eak --propose`. In propose mode
    the harness `dispatch` routes the following before anything else:
    - design tools go to `proposal::propose`. The tool checks `expected_revision` (stale is
      refused) and the session's `max_class`, flattens a `Batch` into steps, and runs every step
      with `command::apply` on a copy, followed by the integrity check (the work `Batch` does). It
      then appends the steps to the session's open proposal. The revision does not move;
    - `Undo`/`Redo` and `rework_apply` are refused (`precondition`), and `design_apply` adds its
      plan's commands to the proposal;
    - reads (except `project_info`, `get_history` and `intel_*`), `run_check`, `run_gates` and
      simulation run on an in-memory copy with the session's open proposal applied
      (`result.proposal_view`). That copy has no folder, so no evidence is stored for a state that
      is not a revision.
  - Storage: `<project>/proposals/<id>.json` (`prop-<12 hex>`) holds `{id, project_id,
    session_id, agent, title, rationale, base_revision, status: open|ready|applied|discarded,
    steps: [{tool, command, class, ids}], created_ms, updated_ms, applied, discarded_by}`.
    `proposals/journal.jsonl` holds one line per event. These are records, not EAK-IR: there is no
    schema bump and no migration. The project file is never touched.
  - Review (`proposal::review`, tool `proposal_status`): the review is recomputed on every call
    and never stored. It holds the effect of each step (its changes, warnings, created ids, and
    `requires`, meaning the earlier steps whose created ids it names) and the total delta of the
    selection. It holds an ERC and DRC `CheckDelta` from the project to the copy: counts
    before/after, findings that appeared or were resolved (by stable finding id), rules newly
    UNKNOWN, `not_implemented` and `limitations`. It holds evidence labels from ADR-0020's `Label`.
    A check is `passed_deterministic_checks` only with 0 errors and no UNKNOWN rule on the copy,
    and `unverified` otherwise (UNKNOWN is never a pass, ADR-0048). The agent's rationale is
    `assumed`. When the selection cannot be applied, `problem` holds the reason (stale, failing on
    the copy, a dropped dependency).
  - Apply (`proposal::apply`, library function; there is **no harness apply tool**):
    - an `ActorKind::Agent` caller is refused;
    - the project must be at `expected_revision` and at the proposal's base revision, else
      `StaleRevision` is returned and nothing is written;
    - a selection must include every step it `requires`;
    - a checkpoint `rNNNNNN-before-<id>` is saved (`Engine::create_checkpoint`), then the chosen
      steps go through `Engine::execute` as one `Batch`: one revision, one undo step. The journal
      actor is the proposing agent (`agent:<name>@<session>`) with an approval naming the person
      and covering the batch's highest class.
  - Object ids: a later step may name an object an earlier step created (the agent was shown its
    id). Re-running the commands would allocate new random ids. `eak_core::with_id_script(replay,
    f)` makes `new_id` replay recorded ids, taken per prefix in order, and record what it hands
    out. Each step stores the ids it allocated on the copy. Every re-run (review, apply) replays
    them, and a re-run that allocates other ids is refused. After the real `Batch`, the allocated
    ids (minus `op_` ids) are compared with the replay; a mismatch is undone at once and reported.
    Outside a script, `new_id` is unchanged.
  - Harness tools in a new `proposal` category: `proposal_begin`, `proposal_status`,
    `proposal_list`, `proposal_submit` and `proposal_discard`. A session discards and submits only
    its own proposals. The agent task ledger ignores proposed edits, because it records what
    happened, not plans.
- **Consequences:**
  - There is no IR, `Command` or schema change and no migration. The harness envelope gains the
    `proposal` category, five tools, `result.proposed`, `result.proposal_view` and the context
    field, so the shared-contract change is recorded here.
  - Standing mode is unchanged: terminal agents keep it unless the user starts them with
    `--propose`.
  - Limitations:
    - propose mode is a policy of the session context, like `max_class`, so it is not a sandbox
      against a hostile agent process that sets its own environment. The co-pilot (S10) will
      start its sessions in propose mode itself;
    - there is no CLI apply: a person applies from the app (S9 dialog, desktop IPC);
    - a proposal does not rebase. Once the project moves on it is stale and must be discarded and
      proposed again;
    - the checks on the copy are ERC and DRC only (MECH/DFM run on the project as usual).
  - Tests: `crates/eak-harness/tests/proposal.rs` is the e2e test through the real `eak` binary.
    It covers four proposed steps (two of them naming parts created by earlier steps), an
    unchanged revision and journal, reads and checks on the copy with no evidence, refused Undo /
    `rework_apply` / wrong revision, the review with dependencies, check deltas and labels, a
    refused partial selection, agent apply refused, stale apply refused with no checkpoint, apply
    giving one revision with the same ids and a checkpoint, one undo taking it all back, a journal
    actor that is the agent, and a stale proposal refusing new steps. `eak-core` `ir.rs`
    `id_script_tests` covers per-prefix replay, recording and the panic reset.
- **Source requirement:** Product spec AI section (AI proposes, the engine verifies, a person
  approves); AGENTS.md non-negotiables (a single write path through `Engine::execute`, AI never
  self-certifies, unknown is never a pass); Traycer review §10 S8 acceptance.
