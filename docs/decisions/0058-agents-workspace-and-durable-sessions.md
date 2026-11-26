# ADR-0058: Agents workspace and durable agent sessions
- **Date:** 2026-11-26
- **Context:** Execution brief §4.1, §9 and §10, goals M2.1 and M2.2. Until now the rail's *Agents*
  item opened Model settings, and the only places that showed agent work were Settings › AI tools
  (sessions of this app run, lost at restart) and Tools › Agent activity (a dialog). The brief asks
  for an Agents workspace separate from provider settings, with a roster, durable sessions, the
  task list/graph, an activity timeline and session detail, all from real state
  (`docs/product/EAK_AGENT_WORKSPACE_SPEC.md`).
- **Options considered:**
  1. Grow Settings › AI tools into the workspace. Rejected: the brief forbids a renamed settings
     page, and settings are modal.
  2. A new agent store (database or service) that the workspace and the agents both write.
     Rejected: a second source of truth beside the WEFT journal (ADR-0053) and the engine journal.
  3. **Chosen:** a top-level app workspace (`src/workspaces/agents.workspace.tsx`, through the
     registry in `shell/registry.ts`) that only **reads** what exists: the WEFT journal, the
     activity timeline, the proposal list and the AI tools' sessions and probes. The one new store
     is the sessions' own index, so they survive a restart.
- **Decision:**
  - **Entry point.** The rail's *Agents* item (both rails: `App.tsx` and `shell/ActivityBar.tsx`)
    opens the Agents workspace in the vision's place (after Library). Model settings stay in the
    Settings menu and get a button in the workspace's context bar.
  - **WEFT read IPC.** `weft_state` (in `agents.rs`, listed in `AGENT_COMMANDS`) replays
    `<project>/weft/journal.jsonl` into agents (with registration time and revision), tasks (with
    creation revision, last status change, unmet dependencies and `ready`) and messages (with
    sequence, time and revision), plus the project's current revision and the count of
    unreadable lines. It writes nothing. No WEFT event, tool or envelope changes.
  - **Durable sessions.** `eak_agents::sessions::Sessions` appends every state change of a session
    to `<app data>/agent-sessions/sessions.jsonl` (one JSON `SessionView` per line; the last line
    of an id wins; a line that does not parse is skipped). The next run replays it:
    - every restored session is marked `from_earlier_run` and loses its terminal tab id and pid,
      because those belong to the earlier run and the terminal engine reuses ids;
    - a session recorded as `running` becomes the new state **`unknown`** with the reason, and that
      reconciliation is appended. EAK never takes a closed app as success (brief §10.5);
    - Stop on a restored session is refused with the reason; its log is read back from its log
      file (headless) or reported as not kept (terminal); Restart starts it again as a new session.
  - **WEFT identity of a session.** `SessionView.weft_id` is the journal actor label
    `agent:<EAK_AGENT_NAME>@<EAK_SESSION_ID>` the session registers under (ADR-0053 launch
    contract): the task id for a headless session, the terminal session id for a tab. The
    workspace joins a session to its WEFT card with it.
  - **Honest message state.** WEFT records sends, not reads. The workspace shows `recorded`, and
    `answered` only where the journal proves it (a verdict from the asked reviewer, a status set by
    a delegation's owner); `read` is "not tracked".
- **Consequences:**
  - `SessionState` gains `unknown` and `SessionView` gains `weft_id` and `from_earlier_run`
    (both `serde(default)`, so earlier index lines read). Shared with the frontend in
    `settings/aiTools.ts`. No IR, `Command`, schema or harness-envelope change, so no migration.
  - One new IPC command (`weft_state`).
  - Limitations, each shown in the UI as unavailable with its reason, not hidden: no chat or send
    to a session (no adapter declares it), no interrupt or resume (M2.8), no task creation from the
    UI (WEFT delegation is agent-to-agent), no embedded PTY (M2.7), no read receipts (M3). The
    workspace polls files every 2-3 s while shown; there is no push from the bus.
- **Tests:** `eak-agents` `sessions::tests` (index replay, running → unknown, restored tail, refused
  stop, no index without a log folder) and `tests/fake_tools.rs` (a real headless run restored
  with its state, WEFT id and log); `src-tauri/tests/agents.rs` (`weft_view` over a real journal
  file); vitest `src/agents/*.test.ts(x)`; ui-evidence `scenarios/m2-agents.json`.
- **Source requirement:** Execution brief §4.1, §9.1-§9.4, §10.1, §10.5; captain's standing rules
  (no decorative controls; Terminal workspace unchanged).
