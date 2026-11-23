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

## Remote branches (2026-11-23)

Firstmate had already deleted the 27 branches fully contained in main. Of the remaining
86 remote branches, these were not touched: `main`, live workers' `fm/eak-dod-*`, this
task's `fm/eak-old-pr-cleanup`, and the head branch of every still-open PR (#1
`fm/eak-dev-method`, #6 `fm/eak-power-calc`, #9 `fm/eak-val-esp32`, #10
`fm/eak-val-power`, #53 `live-test/esp32-devboard`, #88 `fm/eak-p1-e4-proplist`).

Each of the other 71 branches was checked against main in three ways: whether its PR was
merged (squash-landed) with the branch tip unchanged afterwards, whether its commit subjects
are on main, and how many of its added lines (net diff from its merge base) are absent
from main. Lines still missing were read by hand. A deleted branch can be restored from
its tip: `git push origin <tip>:refs/heads/<branch>`.

| Branch | Tip | Verdict | Evidence | Action |
|---|---|---|---|---|
| `fix/windows-quoted-paths` | `e8dae543b3` | COVERED | PR #52: unlanded Windows fixes ported in 8387ff9 | Deleted |
| `fm/eak-ci-uievidence-honest` | `c5c4925f83` | COVERED | PR #81 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-cli-ci` | `a09b9adec2` | COVERED | PR #11: CLI superseded by crates/eak-harness (ADR-0018); recovery block ported in 3c3712b | Deleted |
| `fm/eak-desktop-shell` | `ef8c7af96f` | COVERED | PR #12 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-dev-resources` | `52c091d897` | COVERED | PR #31 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-dev-resources-v013` | `f7285e5b4a` | COVERED | PR #41 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-fab-detail` | `a5154c8c10` | COVERED | PR #5: copper/silk, DRC-014/015, IPC_RULE_VALUES.md on main (21dfe5e, #22) | Deleted |
| `fm/eak-harness` | `8eaa8bd588` | COVERED | PR #50: crates/eak-harness + ADR-0018 on main (e5cee58) | Deleted |
| `fm/eak-ide-providers` | `dc86dc4ae3` | COVERED | PR #47: provider catalog/gemini/sse + ADR-0017 on main (e5cee58) | Deleted |
| `fm/eak-ide-select` | `5c9c127187` | COVERED | PR #48 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-ide-shell` | `40188bb83e` | COVERED | PR #49 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-ide-terminal` | `79b2648ca0` | COVERED | PR #46: crates/eak-terminal + ADR-0019 on main (e5cee58) | Deleted |
| `fm/eak-install-scripts` | `6e1e684446` | COVERED | PR #24 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-intelligence-core` | `cf0600b8dc` | COVERED | PR #3: crates/eak-intelligence on main (5152fab, #16) | Deleted |
| `fm/eak-lcsc-sourcing` | `6f67bf0345` | COVERED | PR #8: sourcing modules on main (15a208f); remaining lines differ only by the LCSC->distributor rename (26e0950) | Deleted |
| `fm/eak-lib-expand` | `defe10c59f` | COVERED | PR #7: same builtin parts + fixture on main (15a208f, #17) | Deleted |
| `fm/eak-o1-editors` | `ad427dca5a` | COVERED | PR #15 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o2-lcsc-jlcpcb` | `8416164522` | COVERED | PR #17 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o3-domain-intel` | `2f098b64e8` | COVERED | PR #18 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o4-model-adaptor` | `ed8345fa39` | COVERED | PR #16 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o5-e2e-flow` | `59f41dae5c` | COVERED | PR #33 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o5-layers-findings-zones` | `c5a8f44320` | COVERED | PR #26 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o5-part4-menus-docs-layout` | `8d1ed4ffab` | COVERED | PR #23 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o5-premium-ui` | `ba538b230a` | COVERED | PR #21 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o5-toolbar-viewport` | `90e1fe73be` | COVERED | PR #27 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-o6-multilayer` | `5a11c575d8` | COVERED | PR #22 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-orchestration-step6-wip` | `edf9b3bd65` | UNIQUE | UNIQUE: task_evaluate.rs and task_templates.rs (role templates, independent evaluator) are not on main; WIP, not ported | Kept |
| `fm/eak-p1-a1-actions` | `ff26c78215` | COVERED | PR #54 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-a2-toolbar` | `7aba48029c` | COVERED | PR #57 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-a3-docks` | `8b6d41f002` | COVERED | eb506e4 on main; follow-ups: 0/5, 2/4, 8/29 and 2/126 added lines missing, all later-reworked e2e scenario/doc text (coveredBy guard is on main) | Deleted |
| `fm/eak-p1-a4-perf` | `8fd35266d8` | COVERED | all 1 commit subjects on main | Deleted |
| `fm/eak-p1-a4-render` | `a0276446eb` | COVERED | all 7 commit subjects on main | Deleted |
| `fm/eak-p1-a5-inspector` | `d0be99a78f` | COVERED | all 3 commit subjects on main | Deleted |
| `fm/eak-p1-b1-ir-v3` | `0a9700ab8c` | COVERED | all 1 commit subjects on main | Deleted |
| `fm/eak-p1-b2-symbols` | `3f73ed46ae` | COVERED | all 2 commit subjects on main | Deleted |
| `fm/eak-p1-b3-editing` | `eb5733d9a4` | COVERED | all 2 commit subjects on main | Deleted |
| `fm/eak-p1-b4-place` | `8b10f59128` | COVERED | PR #84 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-c1-layers` | `b8e0409ba0` | COVERED | all 3 commit subjects on main | Deleted |
| `fm/eak-p1-c2-eco` | `b9b0922f19` | COVERED | PR #74 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-c2-partial` | `836fdfaef3` | COVERED | all 1 commit subjects on main | Deleted |
| `fm/eak-p1-c3-routing` | `2b89cd5215` | COVERED | all 5 commit subjects on main | Deleted |
| `fm/eak-p1-ci-uievidence` | `65bcb22648` | COVERED | PR #55 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-d2-editors` | `964bd41f2e` | COVERED | all 3 commit subjects on main | Deleted |
| `fm/eak-p1-e1-deadcontrols` | `ce3427d1ff` | COVERED | all 1 commit subjects on main | Deleted |
| `fm/eak-p1-e1-deadcontrols-ci` | `4517824ba6` | COVERED | all 2 commit subjects on main | Deleted |
| `fm/eak-p1-e2-atomicity` | `1541965e26` | COVERED | 3/4 commit subjects on main; 3d89a63 (Place click on a symbol) re-implemented on main via downOnObject + pointer-events:none ghost | Deleted |
| `fm/eak-p1-e3-engine-provenance` | `7c407ab339` | COVERED | all 3 commit subjects on main | Deleted |
| `fm/eak-p1-fix-place-selbar` | `2a0b5eea3e` | COVERED | PR #66 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p1-ir-followups` | `86d6fb95ed` | COVERED | all 1 commit subjects on main | Deleted |
| `fm/eak-p2-schematic` | `5e5e906871` | COVERED | Command::Batch + ADR-0013 on main (41d8287, #28) | Deleted |
| `fm/eak-p2-schematic-2` | `15c38bf7a6` | COVERED | PR #58: Batch, MoveWire, MoveNetLabel, selection.ts on main (41d8287, 8b95372) | Deleted |
| `fm/eak-p2-select` | `e2e2466e82` | COVERED | PR #42 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p4-routing` | `e426c9ac20` | COVERED | PR #28 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p4-validation-mfg` | `b327831f4d` | COVERED | PR #30 merged; branch tree equals its merge base 12e104f, which is on main | Deleted |
| `fm/eak-p5-library` | `ecd385cfb1` | COVERED | PR #35 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p6-ai` | `c109cdc363` | COVERED | PR #36 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-p7-simulation` | `5102bfb223` | COVERED | PR #32 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-phase0-research-audit` | `a723db77f7` | COVERED | PR #20 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-qa-e2e` | `a0db3e499b` | COVERED | PR #34 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-qa-e2e-v013` | `8575e2d606` | COVERED | PR #39 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-readme-license-killfix` | `512ab502ba` | COVERED | PR #19 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-release-ci` | `5999319cf1` | COVERED | PR #45 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-release-packaging` | `366c5371f6` | COVERED | PR #29 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-scripts-exec` | `93578e820e` | COVERED | PR #2 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-sr-kernel` | `15c75da961` | COVERED | PR #51 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-stackup` | `05ff8a336e` | COVERED | PR #4: per-layer stackup + ADR-0008 on main (21dfe5e, #22) | Deleted |
| `fm/eak-ui-fixes` | `eae76ac1d7` | COVERED | PR #43 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-version-0.1.2` | `15e5ca1b9d` | COVERED | PR #37 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-version-0.1.3` | `5bbb65edf8` | COVERED | PR #38 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/eak-version-0.1.4` | `a8ae23b1b3` | COVERED | PR #44 merged (squash-landed); branch tip unchanged since merge | Deleted |
| `fm/linux-handoff` | `3c88abfc55` | COVERED | PR #13 merged (squash-landed); branch tip unchanged since merge | Deleted |

Result: 70 branches deleted. One branch was kept: `fm/eak-orchestration-step6-wip`, which holds unique unfinished work (orchestration step 6, role templates and an independent evaluator) that was not ported. 16 branches remain on `origin`.
