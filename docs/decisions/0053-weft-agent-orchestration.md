# ADR-0053: WEFT agent-to-agent orchestration on one project journal
- **Date:** 2026-11-26
- **Context:** The captain's expanded 19:00 scope (remap slice W2-G, Traycer review §10/§11)
  makes agent-to-agent orchestration mandatory. Several agents work on one project: Claude Code
  and Codex sessions (registered through the eak-v2-aitools work), the in-app agent, and any MCP
  client. They must be able to:
  - register;
  - delegate tasks with dependencies;
  - exchange typed messages;
  - route proposals to a reviewer;
  - see conflicts.

  This must use real mechanisms (no fabricated conversation) and leave a traceable history.
  ADR-0051 already guarantees that agent edits are proposals and that only the engineer applies
  one. `task_graph.rs` (ADR-0036) is the typed design-pipeline graph of one run, not a graph of
  delegation between agents, so it is not reused here.
- **Options considered:**
  1. A message broker or daemon process. Rejected:
     - it is a second process to keep alive;
     - it adds a network surface;
     - it is not available to the separate `eak` CLI processes a terminal agent uses.
  2. A table per concern (agents, tasks, messages), each updated in place. Rejected: the history
     would have to be kept separately and could disagree with the state.
  3. **Chosen:** one append-only journal per project, `<project>/weft/journal.jsonl`. The
     registry, the task graph and every inbox are derived by replaying it. Agents use the
     journal only through harness tools, which are also MCP tools.
- **Decision:**
  - **Journal format.** One entry per line:
    `{seq, ts_ms, by, revision, event: agent_registered | task_created | task_status | message, …}`.
    - `by` is the agent's journal actor label (`agent:<name>@<session>`), the same label the
      engine journal uses, so the activity view joins the two.
    - Entries are appended while the caller holds the project engine (the project lock, or the
      app engine through its endpoint), so writes from two agents are serialised.
    - A line that does not parse is counted as `unreadable` and never guessed at.
  - **Tools** (category `weft`, exposed by `eak-mcp`):
    - `weft_register` (tool, model, capabilities);
    - `weft_agents`;
    - `weft_send`;
    - `weft_inbox` (`since` cursor);
    - `weft_tasks`;
    - `weft_conflicts`;
    - `weft_history`.
  - **Messages and their checks** (`weft_send`, typed by `kind`):
    - every sender must be registered, and every recipient must be a registered agent;
    - `delegate` creates a task owned by the recipient, with `depends_on` and an optional parent;
    - `status` (`pending`, `in_progress`, `blocked`, `done`, `rejected`) may be set only by the
      task's owner. `in_progress` and `done` are refused while a dependency is not `done`, so
      execution follows the dependencies;
    - `blocker` marks a task blocked;
    - `context_share` and `finding` need a body;
    - `review_request` names a proposal and a reviewer: an agent other than the proposal's author,
      or `engineer`;
    - `review_verdict` (`approve`, `request_changes`, `reject`) is accepted only from an agent that
      was asked and is not the author. A verdict is advice: Apply still requires the engineer
      (ADR-0051), and no tool applies a proposal.
  - **Conflicts** (`weft_conflicts`):
    - an open proposal made at another revision is `stale`;
    - two open proposals at the current revision whose delta touches the same object (path and
      key from `proposal::review`) `overlap`;
    - each conflict carries how to resolve it.

    Nothing is overwritten silently: once one proposal is applied the others are stale, and
    `proposal::apply` refuses a stale proposal. Resolution is to reject one or to propose again on
    the new revision. EAK does not rebase.
- **Consequences:**
  - There is no IR, `Command` or schema change and no migration. The harness envelope gains the
    `weft` category and seven tools; this ADR records that shared-contract change.
  - Limitations:
    - discovery is registration by the agent's own session (`weft_register`). Detecting installed
      Claude Code or Codex is the AI Tools work (eak-v2-aitools), which registers the sessions it
      starts;
    - the inbox is polled with `since`; there is no push notification;
    - the journal is per project, so agents on different projects do not see each other.
- **Tests:**
  - `crates/eak-harness/tests/weft.rs` runs two real `eak-mcp` processes with different agent
    identities in propose mode and covers:
    - registration;
    - a delegated task with a dependency;
    - out-of-order and non-owner status changes refused;
    - a proposal submitted and sent for review to an agent and to the engineer;
    - a self-review refused and an asked reviewer's verdict delivered;
    - an overlap conflict flagged;
    - the engineer's apply leaving the other proposal stale and refused;
    - the journal history.
  - `weft::tests` round-trips journal lines and replays them into the state.
- **Addendum, activity view (2026-11-26):**
  - `eak_harness::activity::timeline(dir)` builds one time-ordered list. It reads files only and
    writes nothing. Its sources:
    - committed operations of the engine journal (the last line per `op_id`), with the actor,
      the command, the revision before and after, and the changed objects as `path:key`;
    - proposal events (`proposals/journal.jsonl`);
    - WEFT events;
    - saved validation reports (`evidence/*.json` that carry `check`, `revision`, `errors` and
      `warnings`).
  - The desktop IPC `activity_timeline` serves it. Tools › Agent activity… (`ActivityDialog`)
    groups it by actor and re-reads it every 2 s while open, because WEFT messages do not move the
    revision. A revision shows the ERC/DRC errors of the reports saved for its before and after
    revisions; a side with no saved report reads "not run", never 0.
  - The actions use existing paths only:
    - Review proposal opens the S9 review dialog on that proposal (Apply / Discard there);
    - Undo this revision is Edit › Undo, offered only on the newest committed revision;
    - Show on canvas selects and zooms to the first changed symbol or footprint.
  - ui-evidence `scenarios/w2-activity.json` covers it.
  - Limitation: the view polls; there is no push from the bus.
- **Addendum, launch integration (2026-11-26):**
  - Launch contract: a launcher starts `eak-mcp` with these variables set:
    - `EAK_AGENT_NAME` (the tool id);
    - `EAK_SESSION_ID` (its session or task id);
    - `EAK_AGENT_TOOL`, `EAK_AGENT_MODEL` and `EAK_AGENT_CAPABILITIES` (comma-separated).

    The launcher is the AI tools adapter (eak-v2-aitools) or any other.
  - When `EAK_AGENT_TOOL` is set, `eak-mcp` registers the session on the bus itself, through the
    same `weft_register` tool path as a call. It tries after `initialize`, and after each tool
    call while the project does not exist yet. The session's WEFT id is the journal actor label
    `agent:<name>@<session>`, so the activity view shows the registration and everything the
    session does under one actor, live (the view polls every 2 s).
  - A session started without `EAK_AGENT_TOOL` is not registered and calls `weft_register`
    itself.
  - The registration is what the launcher said: EAK does not verify which model a tool runs.
  - Test: `crates/eak-harness/tests/weft.rs`
    `launched_sessions_register_themselves_delegate_review_and_meet_a_stale_conflict`. Two
    launched `eak-mcp` sessions register without a call, delegate, review with a verdict, and
    meet a stale-revision conflict; a session started by hand stays unregistered.
- **Source requirement:** Captain's 19:00 directive §2 (WEFT orchestration); AGENTS.md (one write
  path through `Engine::execute`, AI never self-certifies); ADR-0051.
