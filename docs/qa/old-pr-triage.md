# Old pull request triage (2026-11-23)

Seventeen PRs from earlier sessions were still open: #1, #3-#11, #46, #47, #50, #52, #53,
#58, #69. Each one's diff (against its merge base) was compared with `main` at `2d77753`. The
comparison used file presence, the public symbols each PR adds, and a line-level check of
its added lines against the same files on main. PRs #87-#90 and anything newer than #69 were
not touched. Closing a PR does not delete its branch, so every branch is still on `origin`.

Classification: **COVERED** means equivalent or superseded work is on main. **PARTIAL** means
some real value had not landed. **UNIQUE** means nothing equivalent is on main.

| PR | Title (short) | Class | Where main covers it / what was missing | Action |
|---|---|---|---|---|
| #1 | Ponytail-style method in AGENTS.md | n/a | AGENTS.md change only | **Left open**: changing the project's agent rules is a human-only choice |
| #3 | eak-intelligence provider trait, typed tools, agent loop | COVERED | `crates/eak-intelligence` (agent loop, typed command tools, providers, `remote-http` feature) via `5152fab` (#16) and later work | Closed |
| #4 | Per-layer board stackup, IR v2 | COVERED | `StackupLayer`/`Stackup` in `eak-core/src/ir.rs`, ADR-0008-per-layer-stackup, `21dfe5e` (#22) | Closed |
| #5 | Thermal reliefs, stroke-font silk, DRC-014/015, IPC values | COVERED | `eak-engineering/src/{copper,silk}.rs`, DRC-014/015 in `eak-validation/src/drc.rs`, `docs/engineering/IPC_RULE_VALUES.md`, `21dfe5e` (#22) | Closed |
| #6 | Synchronous-buck calculations, ERC rating checks | PARTIAL | The rating data it reads is on main (builtin parts with `input_voltage_max`, `saturation_current`, `vds_max`, from #7's work), but no ERC consumes it. Its rule IDs ERC-014..016 were later reused on main for the pin-conflict matrix, undriven inputs and power-net checks. The buck loss, capacitor, feedback-divider and E-series calculations are not on main (main has only `buck_duty_cycle` and `buck_inductor_ripple_a`) | **Left open, not ported**: the rating checks need new rule IDs (ERC-022+), which means changing `RULE_FAMILIES` in the IR (`eak-core/src/ir.rs`). Rule IDs and constraint types belong to the rules worker (B5/C5, ADR-0046). The checks' "not checked" info findings would also add a finding to every IC without a rating, which is a product choice |
| #7 | Validation-board parts with datasheet provenance | COVERED | `eak-core/src/builtin.rs` (same 12 parts and ratings) and `tests/fixtures/builtin_m1_parts.json`, `15a208f` (#17) | Closed |
| #8 | Offline LCSC sourcing records, SOURCING report | COVERED | `eak-manufacturing/src/sourcing.rs`, `eak-validation/src/sourcing.rs`, `docs/engineering/sourcing.md` (`15a208f`, #17); fixtures renamed to `tests/fixtures/distributor/` in `26e0950` (ADR-0010) | Closed |
| #9 | ESP32-C3 environment dashboard (validation board) | n/a | Hardware design acceptance board | **Left open**: needs captain authorization for hardware validation |
| #10 | 12-24 V to 5 V / 3 A buck board (validation board) | n/a | Hardware design acceptance board | **Left open**: needs captain authorization for hardware validation |
| #11 | `eak` CLI and three-OS CI matrix | PARTIAL | The CLI is superseded by `crates/eak-harness` (`eak`, `eak-mcp`; ADR-0018, `e5cee58`). The CI matrix, lock error message and process-group kill/drain fixes are on main in other forms. **Missing:** the UNKNOWN_OUTCOME recovery write block was held only in memory, so reopening a project lifted it without any acknowledgement | **Ported**: persisted `recovery.blocked` write block plus regression test (`fix(runtime): persist the UNKNOWN_OUTCOME recovery write block`), then closed |
| #46 | Terminal workspace (PTY engine, UI, notices) | COVERED | `crates/eak-terminal`, `apps/desktop/src/terminal/`, ADR-0019-terminal-workspace, `third_party/licenses/herdr`, `e5cee58` | Closed |
| #47 | Provider catalog, streaming, Gemini, connection test | COVERED | `eak-intelligence/src/provider/{catalog,gemini,sse}.rs`, ADR-0017, `e5cee58` | Closed |
| #50 | Engineering harness: typed tools, eak CLI, MCP server | COVERED | `crates/eak-harness`, `docs/AGENT_HARNESS.md`, ADR-0018, `e5cee58` | Closed |
| #52 | Windows quoted paths, Windows clippy/harness fixes | PARTIAL | The pane-collapse CSS fix and the Windows clippy/test fixes are on main. **Missing:** quoted folder-path input, the `build.ps1`/`install-source.ps1` PowerShell fixes, binding the terminal space to the open project, and Windows foreground-process/agent detection | **Ported** (`fix(windows): port unlanded fixes from old PR #52`), then closed |
| #53 | Live test: ESP32 devboard | n/a | Hardware design live test (also touches desktop/terminal files) | **Left open**: needs captain authorization for hardware validation |
| #58 | Schematic select tool, `Command::Batch` | COVERED | `Command::Batch`, `MoveWire`, `MoveNetLabel` in `eak-runtime/src/command.rs`, ADR-0013-batch-command (`41d8287`, #28), select tool and `tools/schematic/selection.ts` (`8b95372`, #42) | Closed |
| #69 | A5 context properties inspector | COVERED | Same commits landed on main as `473518d`, `fb9451e`, `106f6e8` (`shell/Inspector.tsx`, `shell/inspectorModel.ts`) | Closed |

## Totals
- Closed as COVERED: 10 (#3, #4, #5, #7, #8, #46, #47, #50, #58, #69).
- Ported, then closed: 2 (#11, #52).
- Left open: 5. #1 is an AGENTS.md change, a human-only choice. #9, #10 and #53 are hardware
  acceptance boards that need captain authorization. #6 needs rule IDs owned by the rules
  worker plus a product decision on "not checked" findings.

## Validation
No local build or test was run (captain rule). The ports were validated by CI on the
`fm/eak-old-pr-cleanup` pull request before they were pushed to main. The #52 terminal
warning banner is covered by unit tests (`apps/desktop/src/terminal/projectSpace.test.ts`)
and not by a ui-evidence scenario. That scenario would need two projects and a terminal
space bound to the other one. The ui-evidence harness opens a single project, so it cannot
set that state up.
