# Desktop E2E results

Real-window runs of [`apps/desktop/e2e/run.mjs`](../../apps/desktop/e2e/README.md)
(tauri-driver 2.0 + WebKitWebDriver 2.52 + WebdriverIO 9) against a **debug build** of
`apps/desktop` under Xvfb (1600×1000; app window 1400×900), Linux x86-64. Steps drive the actual
window with real pointer, wheel and keyboard input; pass/fail is read back from the engine
snapshot, the status bar and the files on disk.

## Latest run: 2026-11-15 00:05 IST, v0.1.3 (`main` at `2153054`)

**22 / 23 steps passed.** Screenshots: [`e2e-screenshots-v0.1.3/`](e2e-screenshots-v0.1.3/).
S07 reads connectivity numbers over IPC (no connectivity/netlist view in the UI); every other
step is pure UI. X1–X5 are new: Library, LCSC, JLCPCB/DFM, Simulation, AI assistant.

| Step | Workflow step | Driven via | Result | Evidence | Screenshot |
|---|---|---|---|---|---|
| S01 | Launch app, Project Home visible | UI | **PASS** | home shown; no recent projects (clean profile) | [01-S01.png](e2e-screenshots-v0.1.3/01-S01.png) |
| S02 | Create project (New project form) | UI | **PASS** | project "E2E Demo Board" at r0; folder has .lock, project.eak.json | [02-S02.png](e2e-screenshots-v0.1.3/02-S02.png) |
| S03 | Open schematic editor | UI | **PASS** | schematic view shown; 1 sheet(s) | [03-S03.png](e2e-screenshots-v0.1.3/03-S03.png) |
| S04 | Place two components (Place tool + canvas clicks) | UI | **PASS** | placed R2=R_0603, R1=R_0603 → r2; canvas draws 2 | [04-S04.png](e2e-screenshots-v0.1.3/04-S04.png) |
| S05 | Connect parts (Connect pins tool + Wire tool) | UI | **PASS** | Connect pins R1.2–R2.1 + 3-segment Wire R1.1–R2.2 → r6; 4 wire(s) in IR | [05-S05.png](e2e-screenshots-v0.1.3/05-S05.png) |
| S06 | Update PCB from schematic (Project menu) | UI | **PASS** | 2 footprints on PCB → r7 | [06-S06.png](e2e-screenshots-v0.1.3/06-S06.png) |
| S07 | Connectivity: nets + airwires after sync | IPC (no UI) + UI airwires | **PASS** | 2 nets (N_R2_1, N_R2_2), 2 unrouted, 0 shorts; PCB draws 2 airwires. No netlist/connectivity view in the UI | [07-S07.png](e2e-screenshots-v0.1.3/07-S07.png) |
| S08 | Board outline (Board outline tool, 2 corners, Finish) | UI | **PASS** | outline 4 points, 50.0 × 36.5 mm → r8 | [08-S08.png](e2e-screenshots-v0.1.3/08-S08.png) |
| S09 | Footprint placement (Design → Auto-place footprints) | UI | **PASS** | footprints at 0.0,1.0 / 0.0,-1.0 mm → r9 | [09-S09.png](e2e-screenshots-v0.1.3/09-S09.png) |
| S10 | Layers: set 4 copper layers (PCB Copper layers select) | UI | **PASS** | PCB layer panel shows 4 copper layers → r10 | [10-S10.png](e2e-screenshots-v0.1.3/10-S10.png) |
| S11 | Route (Route → Auto-route all nets) | UI | **PASS** | 10 track segment(s), 2 via(s), 0 unrouted connection(s) → r11 | [11-S11.png](e2e-screenshots-v0.1.3/11-S11.png) |
| S12 | Copper zone (Zone tool, layer + net, 2 corners, Finish) | UI | **PASS** | zone on layer bottom, net N_R2_1 → r12 | [12-S12.png](e2e-screenshots-v0.1.3/12-S12.png) |
| S13 | ERC (toolbar Run ERC) | UI | **PASS** | outcome NO_ERRORS_IN_IMPLEMENTED_CHECKS, 0 error(s), 0 warning(s); lists 4 not_implemented and 2 limitation(s) | [13-S13.png](e2e-screenshots-v0.1.3/13-S13.png) |
| S14 | DRC (toolbar Run DRC) | UI | **PASS** | outcome NO_ERRORS_IN_IMPLEMENTED_CHECKS, 0 error(s), 3 warning(s); lists 7 not_implemented and 6 limitation(s) | [14-S14.png](e2e-screenshots-v0.1.3/14-S14.png) |
| X1 | Library panel: open, search, previews | UI | **PASS** | Library / Parts panel open; query "0603" lists matching entries; 0 symbol/footprint preview(s) rendered | [15-X1.png](e2e-screenshots-v0.1.3/15-X1.png) |
| X2 | LCSC part search (Parts panel, live public catalogue) | UI | **PASS** | LCSC returned 1 row(s) for C25804 | [16-X2.png](e2e-screenshots-v0.1.3/16-X2.png) |
| X3 | JLCPCB prep: DFM check + BOM/DFM tables + bundle | UI | **PASS** | DFM outcome ERRORS (1 err, 0 warn, 7 not_implemented); panel BOM 1 row(s), DFM table 20 row(s); bundle on disk: E2E_Demo_Board-BOM-review.csv, E2E_Demo_Board-JLCPCB-BOM.csv, E2E_Demo_Board-JLCPCB-CPL.csv, E2E_Demo_Board-JLCPCB-gerbers.zip, eak-package, jlcpcb-dfm-report.json, jlcpcb-prep-manifest.json | [17-X3.png](e2e-screenshots-v0.1.3/17-X3.png) |
| X4 | Simulation panel (Simulation menu → Run simulation…) | UI | **FAIL** | WebDriverError: element click intercepted when running "element/node-47929609-4B22-4E73-A181-5AA79777E2C6/click" with method "POST" | [18-X4.png](e2e-screenshots-v0.1.3/18-X4.png) |
| X5 | AI assistant with no model configured | UI | **PASS** | panel states "No AI model is configured"; offline interpreter answered "annotate": annotateOffline command (no AI)Annotate schematic. Assigns reference designators.Annotate schematic~ Annotate AcceptRejectPreviewed on the canvas | [19-X5.png](e2e-screenshots-v0.1.3/19-X5.png) |
| S15 | Generate outputs + manufacturing checks (Export dialog) | UI | **PASS** | 18 files on disk, status DRAFT: E2E_Demo_Board-BOM.csv, E2E_Demo_Board-B_Cu.gbl, E2E_Demo_Board-B_Mask.gbs, E2E_Demo_Board-B_Paste.gbp, E2E_Demo_Board-B_Silkscreen.gbo, E2E_Demo_Board-CPL.csv, E2E_Demo_Board-Edge_Cuts.gm1, E2E_Demo_Board-F_Cu.gtl, …; ERC+DRC+mech reports run inside export (no separate manufacturing-check control) | [20-S15.png](e2e-screenshots-v0.1.3/20-S15.png) |
| S16 | Save (Ctrl+S); engine persists every command | UI | **PASS** | status: "Revision 12: every accepted command is already written (engine reports no blocked writes)."; project folder: .lock, evidence, journal.jsonl, output, project.eak.json | [21-S16.png](e2e-screenshots-v0.1.3/21-S16.png) |
| S17 | Close project (toolbar Close project) | UI | **PASS** | back on Project Home; current_project now errors (engine closed) | [22-S17.png](e2e-screenshots-v0.1.3/22-S17.png) |
| S18 | Reopen from Recent projects and compare | UI | **PASS** | reopened at r12; symbols/footprints/tracks/zones = 2/2/10/1 (identical to before close) | [23-S18.png](e2e-screenshots-v0.1.3/23-S18.png) |

22/23 steps passed.

### Classification

| Area | Verdict | Basis |
|---|---|---|
| Project create / save / close / reopen | **Verified** | S01–S03, S16–S18: reopened revision and object counts identical |
| Schematic place + connect (Connect pins, Wire) | **Verified** | S04–S05 by canvas clicks; needs zoom-in first (O3) |
| PCB update, outline, auto-place, 4 layers, auto-route, zone | **Verified** | S06, S08–S12 |
| Connectivity | **Partially working** | Engine connectivity + airwires work (S07); no netlist/connectivity view in UI |
| ERC / DRC / DFM | **Verified** (as reported, with limits) | Reports list `not_implemented`/`limitations`; DFM shows 1 real error (BOM line without LCSC number) and "not checked" rows |
| Library panel | **Partially working** | Search lists entries (X1); 0 symbol/footprint previews rendered for the query without a selection (not investigated further) |
| LCSC search | **Verified** (live, network) | X2: C25804 returned 1 row from LCSC's public catalogue |
| JLCPCB bundle | **Verified** | X3: BOM, CPL, gerber ZIP and review BOM written to disk; panel says "Prepared for review only. Not manufacturing-ready" |
| Simulation | **Incomplete in split view** | X4 FAIL: panel opens and source controls work, but the **Run simulation** button is covered by the PCB view (click intercepted) |
| AI assistant (no model) | **Verified** | X5: states "No AI model is configured"; offline interpreter returns a preview proposal (Accept/Reject), no invented model reply |
| Manufacturing export | **Verified** | S15: 18 files on disk, status `DRAFT` |

### Findings from this run (for routing; product code not changed)

| # | Finding | Evidence | Owner area |
|---|---|---|---|
| F1 | **Simulation cannot be run from the default split layout**: the bottom-panel Simulation section's *Run simulation* button sits under the PCB view (same overlap as O2 below); WebDriver gets `element click intercepted`. | `18-X4.png` | desktop layout |
| F2 | **Toolbar "Simulate" is disabled** with "Not implemented: no SPICE integration yet" while *Simulation › Run simulation…* opens a working ngspice panel. Contradictory UI. | `18-X4.png` toolbar; `Workspace.tsx` toolbar item `sim` | desktop shell |
| F3 | **Manufacturing side panel overflows**: the JLCPCB DFM table spills past the panel header and the window, pushing the status bar out of the 900 px window. | `18-X4.png`, `21-S16.png` | desktop layout |
| F4 | DRC reports `NO_ERRORS_IN_IMPLEMENTED_CHECKS` with 3 warnings and DRC-008 unchecked (no net currents declarable); see [EVALUATION.md](EVALUATION.md) C1. This run's export stayed `DRAFT` because DFM reported an error. | S14, S15, X3 | `eak-validation` |


Real-window runs of [`apps/desktop/e2e/run.mjs`](../../apps/desktop/e2e/README.md)
(tauri-driver 2.0 + WebKitWebDriver 2.52 + WebdriverIO 9) against a **debug build** of
`apps/desktop` under Xvfb (1600×1000), Linux x86-64. Every step drives the actual window with
real pointer, wheel and keyboard input; pass/fail is read back from the engine snapshot, the
app's status bar and the files on disk. Screenshots: [`e2e-screenshots/`](e2e-screenshots/).

## Previous run: 2026-11-14 18:21 IST, app built from `main` at `fb55118`

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
