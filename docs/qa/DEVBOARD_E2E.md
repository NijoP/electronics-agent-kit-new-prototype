# Dev-board end-to-end run (Claude Code inside EAK) — performance matrix and issues

Run date 2026-11-15, installed app = source build of `f48158d` (Linux x86_64). Raw logs, journal,
check reports and the export folder are kept outside the repo in
`~/.local/state/eak/qa-evidence/devboard/`. Everything below is labelled **genuine EAK engine**
(computed by the running app's engine) or **harness/stand-in** (how the run was driven). Nothing
from the stand-in is counted as an EAK capability.

## Run A — via forwarder, NOT the shipped CLI

The installed bundle shipped no `eak` CLI (only `eak-desktop`), so Claude Code was given a ~30-line
bash+jq+nc script (outside the repo) that forwards the CLI's JSON tool request over
`EAK_HARNESS_SOCKET` to the running app. It computes nothing; every design, check and export result
below came from the real engine through the app's own endpoint. `eak guide` / `eak tools` were
served from local files by the forwarder (see issue H1), not by EAK.

### Driving it (GUI vs. fallback)
| Step | How | Result |
|---|---|---|
| Launch own app instance | `GDK_BACKEND=x11`, private XDG dirs | window up in ~7 s |
| Create project | GUI: AT-SPI click New Project, XTest typing for the name, GTK chooser via AT-SPI | OK, ~38 s incl. driver work |
| Terminal workspace, new space, launcher `claude` | GUI, AT-SPI clicks | OK; PTY env had EAK_PROJECT_DIR/ID/SESSION_ID/HARNESS_SOCKET/TOKEN_FILE |
| Give Claude the brief, relaunch with `--model sonnet --permission-mode dontAsk` | **terminal control API** (`session.run`/`send_input`), not GUI typing | not-via-GUI for this step |

### Stage matrix (genuine engine results, timings from the journal/monitor)
| Stage | Wall | Revisions | Notes |
|---|---|---|---|
| App launch to window | ~7 s | | |
| Brief to first mutation | ~3 min | 0 | 8 failed discovery calls + 10 failed `design_spec_set` calls first |
| Requirements + components + schematic (Batch x3) | ~2 min | 1-3 | 18 symbols, 70 labels, 1 wire |
| Sync schematic to PCB | | 4 | 18 footprints |
| Batches / placement / outline | | 5-8 | outline 4 points |
| AutoRoute / zone / undo churn | ~5 min | 9-19 | 5 AutoRoute calls (2 `nothing_to_do`, 1 `stale_revision`), AddZone then undone |
| Export | | at rev 19 | `output/rev-19/`, Gerber/drill/BOM/CPL/zip |
| Total Claude run | ~8 min wall (turn), 98 session tool calls, 26 failures | 19 | |

Resources (from /proc, 30 s samples): app RSS 127-150 MB, 3-4% CPU; claude RSS ~67-84 MB, ~4% CPU.
No crash.

### Final board (genuine engine)
18 parts (symbols and footprints), 4-point outline, 536 tracks, 41 vias, 0 zones, 1 sheet.
- ERC: `NO_ERRORS_IN_IMPLEMENTED_CHECKS`. MECH: same, but MECH-001 not evaluated (no height limit declared).
- DRC: `ERRORS`, 4 findings, all DRC-004 unrouted nets (IO0, IO1, IO4, UART_TX), 7 `not_implemented`.
- `run_gates jlcpcb_standard`: `required_pads_connected` Fail, `fabricator_capability` Unknown
  (profile is an unverified transcription).
- Export manifest status: **DRAFT** (blocking: 4 DRC errors, MECH-001 unevaluated, required_pads_connected fail).
  The package is not manufacturing-ready and EAK does not claim it is.

Claude stopped at rev 19 without routing the last four nets; it finished its turn on its own.
Whether the placed parts were the requested RP2040/USB-C parts could not be read back from the
terminal (see T2); the journal shows `get_component` x7 and `list_library_components` x2.

## Issues found

| ID | Sev | Issue | Source | State |
|---|---|---|---|---|
| H1 | High | App endpoint answered `guide`/`list_tools`/`design_spec_schema` with "does not run against an open project" when a project is open, so any endpoint/MCP client sending project context could not discover tools (real CLI serves discovery locally and was not affected) | `crates/eak-harness/src/tools.rs` `dispatch` | **Fixed**, main `2778396`, regression test `discovery_tools_answer_while_a_project_is_open` (code-reviewed only) |
| P1 | High | Installed bundle had no `eak` CLI / `eak-mcp`, so Claude Code in an EAK terminal could not reach the harness | installer | fixed upstream in `817e8ff` (other worker); verified in Run B |
| T1 | Med | `strip_ansi` (backing `session.read` / `session.wait`) dropped cursor-forward `CSI n C`, so TUI output (Claude Code) lost word spacing. Plain shell output (printf, tabs, column listings) was verified intact: EAK does not drop spaces, only that read path did | `crates/eak-terminal/src/text.rs` | **Fixed**, main `00dcd7c`, test `cursor_forward_becomes_spaces` |
| T2 | Med | `session.read` of a running Claude Code TUI returns spinner frames only, not the conversation text; an external supervisor cannot read the agent's answers, only the journal | `crates/eak-terminal/src/lib.rs` read | open (needs a screen-grid read instead of raw byte tail) |
| E1 | Med | `design_spec_set` rejected 10 times with `invalid_arguments`; the schema the agent was given (forwarder `tools`) did not show the spec shape, and the error did not say which field was wrong | `crates/eak-harness/src/design.rs` | open; recheck in Run B with real `design spec schema` |
| E2 | Low | `AutoRoute` returned `nothing_to_do` twice and left 4 nets unrouted without telling the agent why (2-layer auto-router limitation is stated only in limitations) | `eak-runtime` AutoRoute | open |
| M1 | Low | MECH-001 height check is skipped unless a height limit is declared; the manifest lists it as a blocking reason, which is correct, but `run_check mech` itself still says `NO_ERRORS_IN_IMPLEMENTED_CHECKS` | `crates/eak-validation/src/mech.rs` | open (already C2 in EVALUATION.md) |
| Q1 | Info | My own monitor polled `get_history` through the forwarder under agent name "claude", inflating Claude's call count by ~50; counts above use the journal for session 4 only | QA tooling | n/a |

## Run B — shipped `eak` CLI, genuine EAK path end to end

Installed from main `2778396` by the source installer, then `8fde70c` (see I1). Claude Code
(`--model sonnet --permission-mode dontAsk`, Write/Edit/web denied, only `eak`/`jq`/`head`/`grep`
Bash allowed) ran in an EAK terminal of a fresh app instance and project, using only the shipped
`eak` on PATH (`/home/dev/.local/bin/eak` -> `current/eak`). Nothing from a forwarder was involved.

Verified in a new EAK terminal with a project open: `command -v eak eak-mcp` found both,
`eak info`, `eak tools` (with the discovery fix) and `eak guide` all worked against the running engine.

How it was driven: project create, Terminal workspace, new space and the **Claude Code launcher
button** were GUI (AT-SPI). The launcher starts bare `claude` as the PTY program, so the flagged
run was started from a shell tab with the terminal control API (not-via-GUI for that step).

### Stage matrix (genuine engine, journal timestamps)
| Stage | Wall | Revisions |
|---|---|---|
| App launch to window | ~8 s | |
| Project + terminal + launcher (GUI) | ~40 s | 0 |
| Claude run (brief to last call) | ~5 min | 21 |
| Idle until Claude stopped by itself | ~7 min | 21 |

Journal: Batch x4, PlaceSymbol, SetBoardOutline, SetNoConnect, SyncSchematicToPcb, AutoPlace,
AutoRoute, AddTrack, AddVia x2, AddZone, AddNetLabel, Undo x5, Redo. 71 session tool calls, 12
failures (AddZone `invalid_arguments` x2, AutoRoute `nothing_to_do` x2, LabelPin `invalid_arguments` x2,
AddTrack `invalid_arguments`, `design_spec_set` `invalid_arguments`, ClearRouting `approval_required`,
Annotate/SetNetVoltage `nothing_to_do`, Batch `not_found`). 4 `export_manufacturing` calls, 8 `verify_routing`.
Resources: app RSS 74-101 MB, ~2% CPU; claude RSS 330-410 MB, 9-38% CPU. No crash.

### Final board (genuine engine, rev 21)
16 parts, 54 labels, 0 wires, 4-point outline, 385 tracks, 37 vias, 1 zone (GND).
- ERC `ERRORS`: 2 errors + 2 warnings. Errors are ERC-003 (nets GND and VBUS feed power-input pins
  with no power-output driver; the design has no USB-C source pin driving VBUS), ERC-016 x2 warnings.
- DRC `ERRORS`: 2 errors (DRC-004: GND and IO4 each have 1 unrouted connection) + 8 warnings
  (DRC-019 x6, DRC-013 x1 zone islands, DRC-020 x1); 7 `not_implemented`.
- MECH: `NO_ERRORS_IN_IMPLEMENTED_CHECKS`, 2 `not_implemented`; MECH-001 not evaluated (no height limit).
- DFM: 6 warnings (DFM-005), 7 `not_implemented`.
- `run_gates jlcpcb_standard`: `required_pads_connected` Fail, `copper_continuity` Fail (zone has 43
  isolated copper islands), `fabricator_capability` Unknown, `electrical_constraints` Unknown (3V3, VBUS
  have no declared current).
- Export manifest status: **DRAFT** (16 files in `output/rev-21/`). Not manufacturing-ready; EAK does not claim it.

Claude again stopped before the board was clean. Compared with Run A it produced a smaller board
(16 vs 18 parts) with different open problems, so the outcome is not repeatable at this model/effort.

## Issues (cumulative, both runs)

| ID | Sev | Issue | Source | State |
|---|---|---|---|---|
| I1 | **High** | Linux source installer exited 0 but never installed `eak`/`eak-mcp`: `install_linux` called `link_cli` without `install_cli`, so `~/.local/bin/eak` pointed at a missing file | `scripts/install-source.sh` `install_linux` | **Fixed**, main `8fde70c`; verified by rerun (`current/` has eak, eak-mcp, eak-desktop). Installer run 1 built in ~5 min with cached deps, no compile errors |
| T1b | Med | The `strip_ansi` fix is incomplete: Claude Code's TUI read back through `session.read` is still word-glued ("Quicksafetycheck:") on the installed `8fde70c` build. The TUI repositions with other cursor codes; stripping cannot recover that. Real fix is reading from a terminal screen grid (a vt emulator), same root cause as T2 | `crates/eak-terminal/src/lib.rs` `read` | open |
| L1 | Low | The "Claude Code" launcher runs bare `claude` as the PTY program: no way to pass model/permission flags, and the tab dies when claude exits (session `completed`, shell gone) | `apps/desktop` agent launcher, `crates/eak-terminal/src/agents.rs` | open |
| C1 | Low | `eak guide \| head` panics "failed printing to stdout: Broken pipe" (`println!` on a closed pipe) | `crates/eak-harness/src/cli.rs` | open |
| E3 | Med | ERC-003 error "nothing drives VBUS/GND": the library has no connector/source pin of power-output type, so a USB-powered board cannot clear ERC without PWR_FLAG-style markers or `SetNetVoltage`; agent tried `SetNetVoltage` (`nothing_to_do`) | `eak-validation/erc.rs`, library | open |
| Q2 | Info | Side effect of running QA with a private `XDG_DATA_HOME`: Claude Code's auto-updater placed its binary under that tmp dir and repointed `~/.local/bin/claude` there; the tmp dir must not be deleted (or the symlink repaired) | QA environment | noted |

H1, T1, P1 above stay as listed: H1 and T1 fixed in source, P1 resolved by I1 and verified.
All fixes are **code-reviewed only** except where verified on the installed build as stated.
