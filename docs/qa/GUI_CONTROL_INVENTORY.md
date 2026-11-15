# GUI control inventory (installed build f48158d, Linux x86_64)

Method: real window driven via AT-SPI actions (semantic) + XTest input, state read from the WebKit accessibility tree and `xwd` screenshots. Evidence is kept outside the repo (`~/.local/state/eak/qa-evidence/`). PASS = the expected state change was observed in the tree; PARTIAL = the action ran but no confirmable state change was observed (or behaviour only partly meets the intent); NOT TESTED = discovered, not exercised.

**Discovered (eligible, enabled) controls: 159** (plus 8 disabled-with-reason items not counted)

- Exercised: 35 → **control coverage 22.0%** (= exercised / eligible)
- PASS 27, PARTIAL 6, FAIL 2, NOT TESTED 124
- **Functional pass rate 77.1%** (= PASS / exercised; PARTIAL not counted as pass)

Not covered: schematic/PCB canvas editing (place, wire, move, route, zones, undo/redo on real geometry), part library placement, copy/paste, multi-select, keyboard shortcuts other than Escape, light theme, Settings model/performance forms, explorer file actions. These were not exercised and are not counted as passing.

| Control | Screen | Kind | Test method | Status | Evidence / observation |
|---|---|---|---|---|---|
| Menu File | EDA menu bar | menu |  | NOT TESTED |  |
| File > New project… | EDA menu | menu item |  | NOT TESTED |  |
| File > Open project… | EDA menu | menu item |  | NOT TESTED |  |
| File > Save | EDA menu | menu item | AT-SPI action + tree diff | PARTIAL | no confirmable state change in tree: document web:EAK; combo box:File |
| File > Save as… | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Not implemented: the engine saves the project in place |
| File > Close project | EDA menu | menu item |  | NOT TESTED |  |
| Menu Edit | EDA menu bar | menu |  | NOT TESTED |  |
| Edit > Undo | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Nothing to undo Ctrl+Z |
| Edit > Redo | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Nothing to redo Ctrl+Shift+Z |
| Edit > Rotate selection | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Select a part first |
| Edit > Delete selection | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Select a part first |
| Edit > Clear selection | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Nothing is selected |
| Menu View | EDA menu bar | menu |  | NOT TESTED |  |
| View > ✓ Side panel | EDA menu | menu item |  | NOT TESTED |  |
| View > ✓ Bottom panel | EDA menu | menu item |  | NOT TESTED |  |
| View > Split schematic and PCB | EDA menu | menu item |  | NOT TESTED |  |
| View > Show Overview | EDA menu | menu item | AT-SPI action + tree diff | PASS | landmark:Project; heading:PROJECT |
| View > Show Explorer | EDA menu | menu item | AT-SPI action + tree diff | PASS | heading:FILES; tool bar:Explorer actions; push button:New file |
| View > Show Library / Parts | EDA menu | menu item | AT-SPI action + tree diff | PASS | entry:Search library; combo box:Category filter; menu item:All categories |
| View > Show PCB Layers | EDA menu | menu item | AT-SPI action + tree diff | PASS | heading:COPPER LAYERS; combo box:Layer count; menu item:2 layers |
| View > Show Domain | EDA menu | menu item | AT-SPI action + tree diff | PASS | landmark:Domain intelligence |
| View > Show Manufacturing | EDA menu | menu item | AT-SPI action + tree diff | PASS | push button:Export Gerbers + drill; landmark:Manufacturing prep; heading:Manufacturing prep · JLCPCB |
| View > Light theme | EDA menu | menu item |  | NOT TESTED |  |
| View > Reset panel layout | EDA menu | menu item |  | NOT TESTED |  |
| Menu Project | EDA menu bar | menu |  | NOT TESTED |  |
| Project > Annotate schematic | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Project; list item:r0 Annotate: nothing to annotate: every symbol alr |
| Project > Update PCB from schematic | EDA menu | menu item |  | NOT TESTED |  |
| Project > Open sheet Sheet 1 | EDA menu | menu item | AT-SPI action + tree diff | PARTIAL | no confirmable state change in tree: document web:EAK; combo box:Project |
| Project > Open PCB | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Project; push button:Run DRC |
| Project > Close all documents | EDA menu | menu item |  | NOT TESTED |  |
| Menu Place | EDA menu bar | menu |  | NOT TESTED |  |
| Place > Part: Resistor 0603 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Capacitor 0603 (MLCC) | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Capacitor 0805 (MLCC) | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: LED 0603 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Pin header 1x02 2.54 mm | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Pin header 1x04 2.54 mm | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: AMS1117-3.3 LDO regulator | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: ATtiny85 microcontroller (SOIC-8) | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Resistor 0402 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Resistor 0805 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Resistor 1206 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Resistor 1210 | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Capacitor 0402 (MLCC) | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Capacitor 1206 (MLCC) | EDA menu | menu item |  | NOT TESTED |  |
| Place > Part: Capacitor 1210 (MLCC) | EDA menu | menu item |  | NOT TESTED |  |
| Menu Design | EDA menu bar | menu |  | NOT TESTED |  |
| Design > Run ERC | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Design; page tab:Findings |
| Design > Run DRC | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Design; heading:DRC · revision 0 |
| Design > Run DFM (JLCPCB standard) | EDA menu | menu item |  | NOT TESTED |  |
| Design > Auto-place footprints | EDA menu | menu item |  | NOT TESTED |  |
| Design > Design rules… | EDA menu | menu item |  | NOT TESTED |  |
| Menu Tools | EDA menu bar | menu |  | NOT TESTED |  |
| Tools > Command palette Ctrl+K | EDA menu | menu item |  | NOT TESTED |  |
| Tools > Go to part, net or sheet Ctrl+P | EDA menu | menu item |  | NOT TESTED |  |
| Tools > Model settings | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Tools; dialog:Model settings |
| Menu Route | EDA menu bar | menu |  | NOT TESTED |  |
| Route > Auto-route all nets | EDA menu | menu item |  | NOT TESTED |  |
| Route > Clear all routing | EDA menu | menu item |  | NOT TESTED |  |
| Route > Interactive routing… | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Use the Route tool in the PCB editor (key 4); not available from the m |
| Menu Simulation | EDA menu bar | menu |  | NOT TESTED |  |
| Simulation > Run simulation… | EDA menu | menu item |  | NOT TESTED |  |
| Menu Manufacturing | EDA menu bar | menu |  | NOT TESTED |  |
| Manufacturing > Export manufacturing files… | EDA menu | menu item |  | NOT TESTED |  |
| Manufacturing > Show manufacturing panel | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Manufacturing; push button:Export Gerbers + drill |
| Manufacturing > Release check… | EDA menu | menu item (disabled) |  | NOT ELIGIBLE (disabled with reason) | Not implemented: exports are always DRAFT |
| Menu Settings | EDA menu bar | menu |  | NOT TESTED |  |
| Settings > Model settings | EDA menu | menu item |  | NOT TESTED |  |
| Settings > Switch to light theme | EDA menu | menu item |  | NOT TESTED |  |
| Settings > Reset panel layout | EDA menu | menu item |  | NOT TESTED |  |
| Menu Help | EDA menu bar | menu |  | NOT TESTED |  |
| Help > Keyboard shortcuts | EDA menu | menu item | AT-SPI action + tree diff | PASS | document web:EAK; combo box:Help; dialog:Keyboard shortcuts |
| Help > About EAK | EDA menu | menu item | AT-SPI action + tree diff | PARTIAL | no confirmable state change in tree: document web:EAK; combo box:Help |
| page tab: Project / EDA | EDA main | page tab |  | NOT TESTED |  |
| page tab: Terminal Workspace | EDA main | page tab |  | NOT TESTED |  |
| push button: New terminal space | EDA main | push button |  | NOT TESTED |  |
| push button: New project | EDA main | push button |  | NOT TESTED |  |
| push button: Open project | EDA main | push button |  | NOT TESTED |  |
| push button: Save | EDA main | push button | AT-SPI action + tree diff | PARTIAL | no confirmable state change in tree: document web:EAK |
| push button: Save as | EDA main | push button |  | NOT TESTED |  |
| push button: Undo | EDA main | push button |  | NOT TESTED |  |
| push button: Redo | EDA main | push button |  | NOT TESTED |  |
| push button: Select tool | EDA main | push button |  | NOT TESTED |  |
| push button: Move tool | EDA main | push button |  | NOT TESTED |  |
| push button: Rotate selection | EDA main | push button |  | NOT TESTED |  |
| push button: Delete selection | EDA main | push button |  | NOT TESTED |  |
| push button: Zoom out | EDA main | push button |  | NOT TESTED |  |
| push button: Zoom in | EDA main | push button |  | NOT TESTED |  |
| push button: Fit to view | EDA main | push button |  | NOT TESTED |  |
| push button: Grid | EDA main | push button |  | NOT TESTED |  |
| push button: Design rules | EDA main | push button |  | NOT TESTED |  |
| push button: Update PCB | EDA main | push button |  | NOT TESTED |  |
| push button: Run ERC | EDA main | push button |  | NOT TESTED |  |
| push button: Simulate | EDA main | push button |  | NOT TESTED |  |
| push button: Commands | EDA main | push button |  | NOT TESTED |  |
| push button: Close project | EDA main | push button |  | NOT TESTED |  |
| toggle button: Overview | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Explorer | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Schematic | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: PCB | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: 3D | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Library / Parts | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: PCB Layers | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Domain | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Manufacturing | EDA main | toggle button |  | NOT TESTED |  |
| push button: Switch to light theme | EDA main | push button |  | NOT TESTED |  |
| push button: Model settings | EDA main | push button |  | NOT TESTED |  |
| push button: New file | EDA main | push button |  | NOT TESTED |  |
| push button: New folder | EDA main | push button |  | NOT TESTED |  |
| push button: Rename | EDA main | push button | AT-SPI action + tree diff | PASS | form:Rename terminal; entry:Rename terminal bash 12; push button:Save |
| push button: Delete | EDA main | push button |  | NOT TESTED |  |
| push button: Refresh | EDA main | push button |  | NOT TESTED |  |
| push button: Collapse all | EDA main | push button |  | NOT TESTED |  |
| push button: project.eak.json Schematic + PCB engine- | EDA main | push button |  | NOT TESTED |  |
| push button: Sheet 1 | EDA main | push button |  | NOT TESTED |  |
| page tab: Sheet 1 | EDA main | page tab |  | NOT TESTED |  |
| push button: Close Sheet 1 | EDA main | push button |  | NOT TESTED |  |
| page tab: PCB | EDA main | page tab |  | NOT TESTED |  |
| push button: Close PCB | EDA main | push button |  | NOT TESTED |  |
| toggle button: Split | EDA main | toggle button |  | NOT TESTED |  |
| push button: Close all | EDA main | push button |  | NOT TESTED |  |
| toggle button: Select | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Place | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Wire | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Connect pins | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Label | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Move | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Delete | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: No connect | EDA main | toggle button |  | NOT TESTED |  |
| toggle button: Pin label | EDA main | toggle button |  | NOT TESTED |  |
| check box: Show grid | EDA main | check box |  | NOT TESTED |  |
| check box: Snap to grid | EDA main | check box |  | NOT TESTED |  |
| page tab: Findings | EDA main | page tab |  | NOT TESTED |  |
| page tab: BOM | EDA main | page tab |  | NOT TESTED |  |
| page tab: Simulation | EDA main | page tab |  | NOT TESTED |  |
| page tab: Log | EDA main | page tab |  | NOT TESTED |  |
| page tab: Tasks | EDA main | page tab |  | NOT TESTED |  |
| push button: Hide bottom panel | EDA main | push button |  | NOT TESTED |  |
| term: + New Space | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | t02-newspace.png: form opened |
| term: Create space | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | t12-created.png: space+bash session created |
| term: Cancel (new space) | Terminal workspace | terminal |  | NOT TESTED |  |
| term: + New terminal | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | bash 11 created |
| term: Shell selector | Terminal workspace | terminal |  | NOT TESTED |  |
| term: pi | Terminal workspace | terminal | AT-SPI action + screenshot | PARTIAL | t13-launch-pi.png: tab "pi" + process pi (stub) running, but started by typing into bash; PTY 24x80 at start |
| term: opencode | Terminal workspace | terminal |  | NOT TESTED |  |
| term: claude | Terminal workspace | terminal | AT-SPI action + screenshot | PARTIAL | t13: same as pi |
| term: codex | Terminal workspace | terminal |  | NOT TESTED |  |
| term: gemini | Terminal workspace | terminal |  | NOT TESTED |  |
| term: Split right | Terminal workspace | terminal | AT-SPI action + screenshot | FAIL | t16-splitright.png: panes stacked vertically, right half blank |
| term: Split down | Terminal workspace | terminal | AT-SPI action + screenshot | FAIL | t18-splitdown.png: 2+1 grid, overflow at small height |
| term: Rename | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | t19/t21: sidebar shows new name |
| term: Restart | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | t20-restart.png: "--- restarted ---" banner |
| term: Close | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | confirm dialog; process count 10->9 |
| term: Close and terminate | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | process terminated, no zombie |
| term: Terminal tab (session) | Terminal workspace | terminal | AT-SPI action + screenshot | PASS | t14/t15: switching keeps scrollback; input reaches the right PTY |
| term: Collapse space | Terminal workspace | terminal |  | NOT TESTED |  |
| term: New terminal space (+) | Terminal workspace | terminal |  | NOT TESTED |  |
| home: Open Project… | Project Home | home |  | NOT TESTED |  |
| home: New Project | Project Home | home | AT-SPI action + screenshot | PASS | form shown |
| home: Choose parent folder… | Project Home | home | AT-SPI action + screenshot | PASS | native chooser opened; path accepted via AT-SPI |
| home: Create (project) | Project Home | home | AT-SPI action + screenshot | PASS | s07-project.png |
| home: Recover and open… | Project Home | home | AT-SPI action + screenshot | PASS | two-step confirm shown |
| home: Confirm: recover and open | Project Home | home | AT-SPI action + screenshot | PASS | project opened (e01) |
| home: Recent project row | Project Home | home |  | NOT TESTED |  |
| home: Reopen last session | Project Home | home |  | NOT TESTED |  |
| home: Sort projects | Project Home | home |  | NOT TESTED |  |
| home: Search projects | Project Home | home |  | NOT TESTED |  |
