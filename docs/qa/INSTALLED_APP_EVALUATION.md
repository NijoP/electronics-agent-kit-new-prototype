# Installed EAK desktop app evaluation (Linux x86_64)

Target: installed source build of commit `f48158d` (main `caa4b71` + one frontend compile fix), v0.1.4, release binary. This is the **first build-verified revision**; the compile fix was fast-forwarded to `main` first. Later main commits (terminal fill `344e0c9`, agent bar `3f37709`, workspace switcher `4435991`, grouped toolbar `198a22c`, empty PCB state `10eee89`) are **code-reviewed only and not in the evaluated binary**; nothing here says they work. The installed binary was not modified, rebuilt or reinstalled.

## A. Environment
GNOME on Wayland with Xwayland, 1920x1080, scale 1.0, 4 CPUs, 7.8 GB RAM. A **separate instance** was run (`GDK_BACKEND=x11`, disposable HOME/XDG dirs, disposable project, PATH holding only stub `pi/opencode/claude/codex/gemini` scripts so no real agent ran; no prompts, no network beyond loopback). Window 1400x900.
Automation that worked: AT-SPI via python `gi` (full WebKit accessibility tree with roles, names, boxes, states and `do_action`, including the native GTK file chooser); XTest via ctypes for keyboard/pointer (focus-guarded to the EAK window); `xwd` on Xwayland for screenshots; `XResizeWindow`; `/proc`. Not available: xdotool, wmctrl, grim, scrot, tauri-driver/WebKitWebDriver, gnome-shell screenshot (AccessDenied); AT-SPI `set_text_contents` is unsupported on web entries, so keyboard input was used.

## B. Current application condition
Reaches an accessible UI in about 7 s. Project Home, New Project (native folder chooser), project creation, the recovery flow ("Previous session interrupted" with a two-step confirm), 12 menus, dialogs (Command palette, Quick open, Design rules, Model settings, Export, Keyboard shortcuts), ERC/DRC/DFM and terminal sessions all responded. Disabled menu items state a reason. Escape did not dismiss the Model settings dialog in this run, and it persisted across workspace switches.

## C. Terminal workspace findings (measured)
| Question | Result |
|---|---|
| Is the viewport undersized? | **Confirmed.** xterm box 1180x360 px (24 rows x 149 cols per `stty size`) in an ~808 px area: **44.6 % height**, 100 % width, vertically centred between two 224 px dark bands. Target is >= 95 %. |
| Cause | Rows stayed at xterm's default 24 while columns fit: the fit addon measures a host whose height is not definite (percentage-height chain `.xterm-box/.xterm-host {height:100%}` through flex ancestors, `apps/desktop/src/styles.css:409` at f48158d). Consistent with fix `344e0c9` (absolute host); not verified in a build. The bands are not an extra pane. |
| Do launchers open the right process? | **Partial.** `claude` and `pi` created tabs with those names and the stub processes ran under those names (status bar: "Working / Waiting for input - foreground: pi"). But it is a **bash session with the command typed in** (bash banner and prompt visible), and the agent's PTY was **24x80 at start** while the pane is 149 cols wide. All five launchers appear because all five CLIs exist on this machine; the "missing CLI" state was not tested live. |
| Do tabs represent sessions? | Yes: names, state dot, "need input" badge; switching keeps scrollback; input reaches the right PTY (unique strings, `stty size` per pane). Tab height 25 px (target 38-44); labels wrap ("bash 5", "+ New terminal") at 1400 px and Split/Rename/Close clip at 1000 px. |
| Splits | **Fail.** Split right gave two panes stacked vertically in the left ~590 px with the right half blank; Split down then gave a 2+1 grid. At 1000x640 the lower pane ran past the status bar. Each PTY matched its own pane width (24x73). |
| Rename / Restart / Close | Pass. Close asks for confirmation; process count 10 to 9, no zombie. Wording "is hung up" is unclear. |
| Workspace navigation prominence | 25 px high, 13 px type, underline-only active state: small. |
| Resize | Window resize reaches the page in ~0.75 s; terminal rows never refit. |

## D. EDA findings
Toolbar: 24 labelled 24 px buttons in two rows plus 12 menus; Undo/Rotate/Delete disabled until selection (reason shown in menus, not on the buttons). Explorer: file names truncate ("proj...") behind badges; the Explorer activity button collapses the side panel. Schematic: "An empty design" card; the grid area does not fill the pane. PCB (empty project): the canvas element fills the pane (1076x497) but the drawn board grid spans ~620 px, offset left, ruler 0..100 mm. With no outline that is expected, but the card ("open Library / Parts") is the wrong guidance for the PCB. Canvas editing (place, wire, route, zones, undo) was **not exercised**.

## E. Control coverage
[GUI_CONTROL_INVENTORY.md](GUI_CONTROL_INVENTORY.md): 159 eligible controls, 35 exercised (22.0 %), 27 pass, 6 partial, 2 fail, 124 not tested; functional pass rate 77.1 % of exercised. These figures describe this run's depth, not product quality.

## F. Security
[SECURITY_ASSESSMENT.md](SECURITY_ASSESSMENT.md): no Critical/High confirmed; one Medium (environment inheritance), Low/Info items; dependency audit not run.

## G. Performance
[PERFORMANCE_OBSERVATIONS.md](PERFORMANCE_OBSERVATIONS.md).

## H. Scorecard (evaluator judgment, installed f48158d only; security reported separately, not averaged)
| Area | Weight | Score | Why |
|---|---:|---:|---|
| Terminal workspace and agent usability | 25 | 9 | 44.6 % viewport, broken splits, small tabs, typed-into-bash launch |
| EDA workspace usability | 15 | 8 | works but dense/small; little exercised; weak PCB empty guidance |
| Functional controls and workflows | 20 | 11 | 27/35 exercised pass; 78 % untested |
| Visual design and hierarchy | 15 | 8 | consistent theme; small targets, clipping |
| Performance and responsiveness | 10 | 7 | idle 0.2 % CPU, 410-525 MB; slow open/recover noted |
| Accessibility and interaction consistency | 5 | 3 | good accessibility tree; Escape inconsistency |
| Reliability and recovery | 10 | 7 | recovery flow, confirm-on-close, clean teardown |
| **Total** | 100 | **53** | |

## I. Fixes
No source fix from this evaluation (no confirmed P0 security or data-loss defect). Terminal and toolbar defects are already addressed on main by the UX worker (code-reviewed only). `f48158d` landed on main. Remaining: [REMEDIATION_LEDGER.md](REMEDIATION_LEDGER.md).

## J. Installed binary
Untouched; only a disposable second instance was run.

## K. GitHub
Branch `fm/eak-gui-eval`, landed on main by fast-forward.
