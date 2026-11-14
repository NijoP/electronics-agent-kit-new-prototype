# Desktop E2E results

Real-window runs of [`apps/desktop/e2e/run.mjs`](../../apps/desktop/e2e/README.md)
(tauri-driver 2.0 + WebKitWebDriver 2.52 + WebdriverIO 9) against a **debug build** of
`apps/desktop` under Xvfb (1600×1000), Linux x86-64. Every step drives the actual window with
real pointer, wheel and keyboard input; pass/fail is read back from the engine snapshot, the
app's status bar and the files on disk. Screenshots: [`e2e-screenshots/`](e2e-screenshots/).

## Latest run: 2026-11-14 18:21 IST, app built from `main` at `fb55118`

**18 / 18 steps passed** (17 through the UI; S07 reads connectivity numbers over IPC because
the UI has no connectivity/netlist view; its airwires are checked in the UI).

| Step | Workflow step | Driven via | Result | Evidence | Screenshot |
|---|---|---|---|---|---|
| S01 | Launch app, Project Home visible | UI | **PASS** | home shown; no recent projects (clean profile) | [01-S01.png](e2e-screenshots/01-S01.png) |
| S02 | Create project (New project form) | UI | **PASS** | project "E2E Demo Board" at r0; folder has .lock, project.eak.json | [02-S02.png](e2e-screenshots/02-S02.png) |
| S03 | Open schematic editor | UI | **PASS** | schematic view shown; 1 sheet(s) | [03-S03.png](e2e-screenshots/03-S03.png) |
| S04 | Place two components (Place tool + canvas clicks) | UI | **PASS** | placed R2=R_0603, R1=R_0603 → r2; canvas draws 2 | [04-S04.png](e2e-screenshots/04-S04.png) |
| S05 | Connect parts (Connect pins tool + Wire tool) | UI | **PASS** | Connect pins R1.2–R2.1 + 3-segment Wire R1.1–R2.2 → r6; 4 wire(s) in IR | [05-S05.png](e2e-screenshots/05-S05.png) |
| S06 | Sync schematic to PCB (Project menu) | UI | **PASS** | 2 footprints on PCB → r7 | [06-S06.png](e2e-screenshots/06-S06.png) |
| S07 | Connectivity: nets + airwires after sync | IPC (no UI) + UI airwires | **PASS** | 2 nets (N_R2_1, N_R2_2), 2 unrouted, 0 shorts; PCB draws 2 airwires. No netlist/connectivity view in the UI | [07-S07.png](e2e-screenshots/07-S07.png) |
| S08 | Board outline (Board outline tool, 2 corners, Finish) | UI | **PASS** | outline 4 points, 50.0 × 36.5 mm → r8 | [08-S08.png](e2e-screenshots/08-S08.png) |
| S09 | Footprint placement (Design → Auto-place footprints) | UI | **PASS** | footprints at 0.0,1.0 / 0.0,-1.0 mm → r9 | [09-S09.png](e2e-screenshots/09-S09.png) |
| S10 | Layers: set 4 copper layers (PCB Copper layers select) | UI | **PASS** | PCB layer panel shows 4 copper layers → r10 | [10-S10.png](e2e-screenshots/10-S10.png) |
| S11 | Route (Route → Auto-route all nets) | UI | **PASS** | 10 track segment(s), 2 via(s), 0 unrouted connection(s) → r11 | [11-S11.png](e2e-screenshots/11-S11.png) |
| S12 | Copper zone (Zone tool, layer + net, 2 corners, Finish) | UI | **PASS** | zone on layer bottom, net N_R2_1 → r12 | [12-S12.png](e2e-screenshots/12-S12.png) |
| S13 | ERC (toolbar Run ERC) | UI | **PASS** | outcome NO_ERRORS_IN_IMPLEMENTED_CHECKS, 0 error(s), 0 warning(s); lists 4 not_implemented and 2 limitation(s) | [13-S13.png](e2e-screenshots/13-S13.png) |
| S14 | DRC (toolbar Run DRC) | UI | **PASS** | outcome NO_ERRORS_IN_IMPLEMENTED_CHECKS, 0 error(s), 3 warning(s); lists 7 not_implemented and 4 limitation(s) | [14-S14.png](e2e-screenshots/14-S14.png) |
| S15 | Generate outputs + manufacturing checks (toolbar Export Gerbers) | UI | **PASS** | 16 files on disk, status INTERNALLY_VERIFIED_RELEASE_CANDIDATE: E2E_Demo_Board-BOM.csv, E2E_Demo_Board-B_Cu.gbl, E2E_Demo_Board-B_Mask.gbs, E2E_Demo_Board-B_Paste.gbp, E2E_Demo_Board-B_Silkscreen.gbo, E2E_Demo_Board-CPL.csv, E2E_Demo_Board-Edge_Cuts.gm1, E2E_Demo_Board-F_Cu.gtl, …; ERC+DRC+mech reports run inside export (no separate manufacturing-check control) | [15-S15.png](e2e-screenshots/15-S15.png) |
| S16 | Save (Ctrl+S); engine persists every command | UI | **PASS** | status: "Saved: every accepted command is already on disk (revision 12)."; project folder: .lock, evidence, journal.jsonl, output, project.eak.json | [16-S16.png](e2e-screenshots/16-S16.png) |
| S17 | Close project (toolbar Close project) | UI | **PASS** | back on Project Home; current_project now errors (engine closed) | [17-S17.png](e2e-screenshots/17-S17.png) |
| S18 | Reopen from Recent projects and compare | UI | **PASS** | reopened at r12; symbols/footprints/tracks/zones = 2/2/10/1 (identical to before close) | [18-S18.png](e2e-screenshots/18-S18.png) |

18/18 steps passed.

## Observations from the real window (for routing)

| # | Observation | Evidence | Owner area |
|---|---|---|---|
| O1 | The export at S15 was promoted to `INTERNALLY_VERIFIED_RELEASE_CANDIDATE` with DRC at 3 warnings. No net in this design has a declared current, so DRC-008 checked nothing (see [EVALUATION.md](EVALUATION.md) C1/C2). The run confirms the gate behaviour; it does not claim the board is wrong. | `output/rev-12/manifest.json` gates: ERC 0/0, DRC 0 err / 3 warn, MECH 0/0, `blocking_reasons: []` | `eak-validation`, `eak-manufacturing` |
| O2 | In split view the PCB view is drawn over the bottom panel: the activity log is visible only under the schematic half. | `e2e-screenshots/12-S12.png` | desktop layout |
| O3 | At fit-to-sheet zoom the schematic canvas is ~330 px wide for an A4 sheet; a 0603 resistor's pins are ~2 px apart and the pin-snap radius is ~1.5 px, so pins cannot practically be hit without zooming first. | run 2, S05 | desktop schematic UX |
| O4 | With the Board outline tool active, the PCB tool-options row wraps and the `Copper layers` select was overlapped by the canvas (`element click intercepted`) at a 1400×900 window. Not seen once the Select tool is active. | run 2, S10 | desktop layout |
| O5 | `ConnectPins` correctly refuses a connection whose every L-shaped route crosses another pin and says so in the status bar; the harness draws that connection with the Wire tool instead. | run 3 status bar text | (works as designed) |

## Run history (honest record)

| Run | Time (IST) | App build | Result | Notes |
|---|---|---|---|---|
| 1 | 18:01 | `fb55118` | 0/1 | `browserName: "wry"` capability rejected by WebKitWebDriver (harness) |
| 2 | 18:03 | `fb55118` | 12/18 | Harness: point fields read as arrays (NaN), outline drawn in canvas fractions; O3, O4 seen |
| 3 | 18:13 | `fb55118` | 14/18 | Pins hit after wheel zoom; `ConnectPins` refused the outer pins (O5); cascade to route/zone |
| 4 | 18:17 | `fb55118` | 17/18 | Connectivity checked before PCB sync (engine connectivity is PCB copper) |
| 5 | 18:21 | `fb55118` | **18/18** | Table above |

A post-freeze run on the latest `main` is appended below when its build completes.
