# ADR-0036: Agent task phases, the PCB synchronisation gate and shared task status

Status: accepted (2026-11-16)

## Context
ADR-0031 tells the editor that a terminal agent moved the revision, but nothing says what the
agent is doing. Terminal output is not proof that a design change was committed, an agent could
run `SyncSchematicToPcb` before any ERC, and the terminal and EDA workspaces had no common view
of the revision or task state.

## Decision
* **Task ledger (new shared contract):** `<project>/agent-tasks.json`
  (`crates/eak-harness/src/agent_task.rs`): `AgentTask` with `phase`, `starting_revision`,
  latest `revision`, `last_erc_errors`, `last_drc_errors`, `ended_ms` and a phase `history`.
  Phases: `planning`, `generating_schematic`, `schematic_committed`, `running_erc`,
  `schematic_approved`, `syncing_pcb`, `placement`, `routing`, `running_drc`,
  `manufacturing_preview`, and the endings `completed`, `partial`, `blocked`, `failed`, `cancelled`.
* **Grounded, not declared:** phases advance only from harness call outcomes (a committed
  command that moved the revision, a finished `run_check`, a refused gate, a compiled export).
  `schematic_approved` needs an ERC with 0 errors on committed symbols. A later schematic edit
  withdraws it. `blocked` is the state after a refused gate and the next grounded call replaces it.
* **Ending a task:** new tools `agent_task_begin`, `agent_task_finish`, `agent_task_status`
  (category `task`). `completed` is granted only when the evidence supports it (ERC-clean
  schematic; after any PCB stage a DRC with 0 errors since the last board change); otherwise the
  task ends `partial` with the reason. The agent cannot self-certify.
* **Gate:** the harness refuses `SyncSchematicToPcb` (also inside a `Batch`) with `precondition`
  and `details.gate = "pcb_sync"` unless the schematic has committed symbols and stored ERC
  evidence exists with no `schematic`/`library` change journalled after it. ERC errors do not
  block the sync (the engineer decides) but are recorded and prevent `completed`. The desktop
  "Update PCB" action runs ERC first when none exists for the revision on screen.
* **Shared status:** the desktop emits `eak://agent-task-changed` (`AgentTasksChanged`: project
  dir + tasks) when a harness call changed the ledger, and serves `agent_tasks` over IPC. The EDA
  status bar and the Terminal workspace both render `taskSummary(task, revision)`.
* **Refresh keeps the view:** the revision refresh replaces the snapshot only; per-document camera
  lives in `shell/viewState.ts` and is untouched, and selection is kept unless the selected object
  no longer exists (`reconcileSelection`).

## Consequences
* New tool category `task`; MCP/CLI list it. No change to `Command`, `CommandResponse` or the
  project file format; the ledger is a progress record and never evidence for a check.
* Not covered: the autonomous design flow and rework tools apply plans through their own
  validation and do not pass through this gate; UI-originated edits do not create tasks.
* Status: code-reviewed only, CI pending. Tests: `agent_task.rs` unit tests, the gate assertions
  in `crates/eak-harness/tests/e2e.rs`, `apps/desktop/src/agentTasks.test.ts`.
