# Desktop E2E results

Real-window run of `apps/desktop/e2e/run.mjs` (tauri-driver 2 + WebKitWebDriver 2.52 +
WebdriverIO 9) against a **debug build** of `apps/desktop` under Xvfb (1600×1000), Linux.
Every step drives the actual window; pass/fail is read back from the engine snapshot and the files
on disk. Screenshots: [`e2e-screenshots/`](e2e-screenshots/) (one per step).

## Run 1: 2026-11-14 18:03 IST, `main` at `fb55118` (pre-freeze)

**12 / 18 steps passed.** Raw table as produced by the harness:

| Step | Workflow step | Driven via | Result | Evidence | Screenshot |
|---|---|---|---|---|---|
| S01 | Launch app, Project Home visible | UI | **PASS** | home shown; no recent projects (clean profile) | [01-S01.png](e2e-screenshots/01-S01.png) |
| S02 | Create project (New project form) | UI | **PASS** | project "E2E Demo Board" at r0; folder has .lock, project.eak.json | [02-S02.png](e2e-screenshots/02-S02.png) |
| S03 | Open schematic editor | UI | **PASS** | schematic view shown; 1 sheet(s) | [03-S03.png](e2e-screenshots/03-S03.png) |
| S04 | Place two components (Place tool + canvas clicks) | UI | **PASS** | placed R2=R_0603, R1=R_0603 → r2; canvas draws 2 | [04-S04.png](e2e-screenshots/04-S04.png) |
| S05 | Connect pins (Connect pins tool, two pin pairs) | UI | **FAIL** | ConnectPins was not accepted | [05-S05.png](e2e-screenshots/05-S05.png) |
| S06 | Connectivity / netlist | IPC (no UI) | **FAIL** | expected ≥2 nets, got 0 | [06-S06.png](e2e-screenshots/06-S06.png) |
| S07 | Sync schematic to PCB (Project menu) | UI | **PASS** | 2 footprints on PCB → r3 | [07-S07.png](e2e-screenshots/07-S07.png) |
| S08 | Board outline (Board outline tool, 2 corners, Finish) | UI | **PASS** | outline with 4 points → r4 | [08-S08.png](e2e-screenshots/08-S08.png) |
| S09 | Footprint placement (Design → Auto-place footprints) | UI | **FAIL** | AutoPlace not accepted | [09-S09.png](e2e-screenshots/09-S09.png) |
| S10 | Layers: set 4 copper layers (PCB Copper layers select) | UI | **FAIL** | WebDriverError: element click intercepted when running "element/node-26F887C1-DF43-416A-92F3-F00B7E0EC6F8/click" with method "POST" | [10-S10.png](e2e-screenshots/10-S10.png) |
| S11 | Route (Route → Auto-route all nets) | UI | **FAIL** | AutoRoute not accepted | [11-S11.png](e2e-screenshots/11-S11.png) |
| S12 | Copper zone (Zone tool, layer + net, 2 corners, Finish) | UI | **FAIL** | no nets offered for the zone | [12-S12.png](e2e-screenshots/12-S12.png) |
| S13 | ERC (toolbar Run ERC) | UI | **PASS** | outcome ERRORS, 4 error(s), 0 warning(s); lists 4 not_implemented and 2 limitation(s) | [13-S13.png](e2e-screenshots/13-S13.png) |
| S14 | DRC (toolbar Run DRC) | UI | **PASS** | outcome ERRORS, 4 error(s), 0 warning(s); lists 7 not_implemented and 4 limitation(s) | [14-S14.png](e2e-screenshots/14-S14.png) |
| S15 | Generate outputs + manufacturing checks (toolbar Export Gerbers) | UI | **PASS** | 14 files on disk, status DRAFT: E2E_Demo_Board-BOM.csv, E2E_Demo_Board-B_Cu.gbl, E2E_Demo_Board-B_Mask.gbs, E2E_Demo_Board-B_Paste.gbp, E2E_Demo_Board-B_Silkscreen.gbo, E2E_Demo_Board-CPL.csv, E2E_Demo_Board-Edge_Cuts.gm1, E2E_Demo_Board-F_Cu.gtl, …; ERC+DRC+mech reports run inside export (no separate manufacturing-check control) | [15-S15.png](e2e-screenshots/15-S15.png) |
| S16 | Save (Ctrl+S); engine persists every command | UI | **PASS** | status: "Saved: every accepted command is already on disk (revision 4)."; project folder: .lock, evidence, journal.jsonl, output, project.eak.json | [16-S16.png](e2e-screenshots/16-S16.png) |
| S17 | Close project (toolbar Close project) | UI | **PASS** | back on Project Home; current_project now errors (engine closed) | [17-S17.png](e2e-screenshots/17-S17.png) |
| S18 | Reopen from Recent projects and compare | UI | **PASS** | reopened at r4; symbols/footprints/tracks/zones = 2/2/0/0 (identical to before close) | [18-S18.png](e2e-screenshots/18-S18.png) |

12/18 steps passed.

### Triage of the failures

| Step | Cause | Verdict |
|---|---|---|
| S05 Connect pins | The schematic canvas fits the whole A4 sheet into ~330 px, so a 0603 resistor's two pins are ~2 px apart and the pin-snap radius is ~1.5 px. The harness clicked pin centres at that scale and the clicks missed the pins (no `ConnectPins` sent). | **Harness** (needs to zoom in before clicking pins). The UI itself makes tiny parts hard to hit at fit-to-sheet zoom. |
| S06 Connectivity | Consequence of S05 (no connections → 0 nets). Connectivity IPC answered correctly. | Cascade |
| S08 → S09 Auto-place | S08 passed, but the outline corners were taken as fractions of a PCB canvas zoomed onto one footprint, giving a board a few mm wide; `AutoPlace` was then rejected (footprints do not fit). | **Harness** (outline must be drawn in board mm) |
| S10 Layer count | `element click intercepted`: with the Board outline tool still active, the PCB tool-options row wraps and the `Copper layers` select is overlapped by the canvas at a 1400×900 window in split view (see `09-S09.png`). | **Harness ordering + possible UI layout bug** (to confirm) |
| S11 Auto-route, S12 Zone | Consequence of S05/S09 (no nets, no placement). | Cascade |

What this run does establish on real UI: project create, schematic open, part placement by canvas
click, schematic→PCB sync, board-outline tool, ERC and DRC from the toolbar (both report
`not_implemented` and `limitations`), export of 14 manufacturing files to disk marked `DRAFT`,
save confirmation, close, and reopen from Recent with identical revision and object counts.

Run 2 (post-freeze, latest `main`, harness geometry fixed) is recorded below when complete.
