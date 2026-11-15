# Pro EDA workspace implementation matrix (2026-11-16)

Built from the code in this tree, not from plans. It reuses
[CAPABILITY_GAP_MATRIX](../development/CAPABILITY_GAP_MATRIX.md) (state letters),
[ADR-0031](../decisions/0031-revision-changed-event.md) (live sync) and
[ADR-0035](../decisions/0035-agent-task-phases-and-pcb-sync-gate.md) (task phases and gate) rather
than re-deriving them. Research basis:
[EASYEDA_EDA_INTERACTION_AUDIT](../research/EASYEDA_EDA_INTERACTION_AUDIT.md).

**States:** V verified by a run · U implemented, not verified (code-reviewed; CI pending) ·
P partial · M missing. Nothing here is V unless GitHub Actions showed it. No local build or run
produced this document. Every design mutation is `Engine::execute` with a typed `Command`.

| Feature | State | Source | Backend command / tool | IPC | UI component | Tests | Limitations |
|---|---|---|---|---|---|---|---|
| Project open/create/recent | U | `src-tauri/src/lib.rs` | engine open/create | `open_project`, `create_project`, `recent_projects` | `ProjectHome.tsx`, `shell/ProjectDialogs.tsx` | `tests/ipc.rs`, `projectHome.test.ts` | never seen in a live window |
| Project tree / file explorer | U | `src-tauri/src/workspace.rs` | read-only file ops | `project_files`, `project_file_*` | `shell/FileExplorer.tsx`, `shell/ProjectOverview.tsx` | `tests/workspace.rs`, `projectOverview.test.ts` | no document-type tree beyond files and sheets |
| Schematic editing | U | `eak-runtime/command.rs` | `PlaceSymbol`, `AddWire`, `ConnectPins`, `LabelPin`, `MoveSymbol`... | `execute_command` | `SchematicView.tsx`, `tools/schematic/*` | `twoResistors.test.tsx`, `SchematicSelect.test.tsx`, `editors.test.tsx` | wiring/bus tools limited to existing commands |
| Schematic generation by agent | U | `eak-harness/src/tools.rs`, `design.rs` | design tools, `design_*` flow | harness socket | rendered live by `SchematicView` | `crates/eak-harness/tests/e2e.rs` | netlist import with diagnostics not built (Milestone 2) |
| Terminal-to-EDA live sync | U | `src-tauri/src/sync.rs`, `lib.rs::harness_call` | harness tools on the app engine | event `eak://revision-changed` | `Workspace.tsx`, `revisionSync.ts` | `tests/live_sync.rs`, `liveSync.test.tsx` | 1.5 s poll fallback remains |
| Agent task phases | U | `eak-harness/src/agent_task.rs` | `agent_task_begin/finish/status` | `agent_tasks`, event `eak://agent-task-changed` | `StatusBar.tsx`, `TerminalWorkspace.tsx`, `agentTasks.ts` | unit tests in `agent_task.rs`, `agentTasks.test.ts`, `e2e.rs` | phases only from harness calls; UI edits create no tasks |
| ERC gate before PCB sync | U | `agent_task.rs::pcb_sync_gate`, `tools.rs::design` | `SyncSchematicToPcb` | `execute_command` (UI runs ERC first) | `Workspace.tsx::syncPcb` | `e2e.rs`, `flow.test.tsx` test 3 | autonomous flow/rework apply paths do not use the gate |
| Selection and camera across refresh | U | `revisionSync.ts`, `shell/viewState.ts` | none | none | `Workspace.tsx` | `agentTasks.test.ts` | dangling selection dropped only for symbols/footprints |
| Schematic-to-PCB sync + report | U | `command.rs::SyncSchematicToPcb`, `shell/syncReport.ts` | `SyncSchematicToPcb`, `operation_changes` | `operation_changes` | `shell/SyncReportDialog.tsx` | `syncReport.test.ts`, `flow.test.tsx` | no rename detection |
| PCB editing | U | `command.rs` | `MoveFootprint`, `AddTrack`, `AddVia`, `AddZone`, `SetBoardOutline` | `execute_command` | `PcbView.tsx`, `routeTool.ts`, `tools/pcb/*` | `routeTool.test.ts`, `PcbSelect.test.tsx`, `outline.test.ts` | interactive push/shove routing missing |
| Auto place / route | U | `eak-engineering/placer.rs`, `router.rs` | `AutoPlace`, `AutoRoute`, `ClearRouting` | `execute_command` | PCB toolbar | engine tests | no progress/cancel stream; own router, no external dependency |
| Layer manager | U | `shell/LayerManager.tsx`, `layers.ts` | `SetStackup`, `SetLayerCount` | `execute_command` | `LayerManager.tsx` | `layers.test.ts`, `layers-findings.test.tsx` | |
| Design rules / DRC / ERC / DFM | U | `eak-validation`, `shell/RulesDialog.tsx` | `run_check`, `SetDesignRules` | `run_check` | Findings tab, `RulesDialog.tsx` | `rules.test.tsx` | unknown/unimplemented checks listed, never passed |
| Manufacturing export | U | `eak-manufacturing` | `export_manufacturing` | `export_manufacturing` | `shell/ExportDialog.tsx` | `tests/ipc.rs` | draft only, never "manufacturing-ready" |
| 3D viewer | U | `viewer3d/` | read-only | none | `workspaces/viewer3d.workspace.tsx` | `scene.test.ts`, `modelLoader.test.ts` | never rendered in WebGL here |
| Terminal sessions | U | `crates/eak-terminal`, `src-tauri/src/terminal.rs` | PTY control API | `TERMINAL_IPC` | `terminal/TerminalWorkspace.tsx` | `terminal.test.tsx`, crate PTY tests | split/restart behaviour unrun |
| Agent harness (CLI/MCP) | U | `crates/eak-harness` | all tools | socket + stdio | none | `tests/e2e.rs` | no agent has driven a full design |
| Workspace tabs (EDA / Terminal / +) | U | `shell/workspaces.ts`, `workspaces/*` | none | none | `ProjectTabs.tsx`, registry | `shell.test.tsx` | |
| Library, supplier, datasheet | n/a | owned by another lane | | | | | not touched by this work |
| Gerber viewer, impedance rules | M | | | | | | audit lists patterns only |

Next by directive priority: (2) netlist-to-schematic generation with import diagnostics,
(3) schematic-to-PCB sync progress, (4) schematic workspace interaction gaps, (5) PCB workspace,
(6) shared terminal sessions. Disabled-with-reason is the rule for any tool not yet implemented.
