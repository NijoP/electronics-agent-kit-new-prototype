# Real-window UI evidence (GitHub Actions)

Every UI slice ends with screenshots of the real app, taken in CI (no local builds). The
`ui-evidence` workflow (`.github/workflows/ui-evidence.yml`) builds a debug build of
`apps/desktop`, starts it under Xvfb with `GDK_BACKEND=x11` (the setup of
[INSTALLED_APP_EVALUATION.md](INSTALLED_APP_EVALUATION.md) section A), drives a scenario with
real X input and uploads the screenshots.

- **Input** is real X input through XTest (`xdotool`), at the centre of the target element.
- **Screenshots** are of the whole X screen (`import -window root`), so they show the real
  window, not a webview render.
- **WebDriver** (`tauri-driver` + `WebKitWebDriver`) only reads the DOM: where an element is,
  and waiting until the expected state is visible.
- **Fixture**: the `fixture-project` step creates an isolated software project in the runner's
  temp folder through the same IPC the UI uses (`create_project`), then the scenario opens it
  through the UI. It is labelled `SETUP` in the results. No real hardware design is used, and
  app data (recent list, layout) lives under the run's own folder.

## When it runs, and its status
On pull requests that touch `apps/desktop/**` (or the workflow), on pushes to `main` with the
same paths, and by hand (`workflow_dispatch`). It is a **separate workflow with
`continue-on-error: true`** and is not a required check until it has proven stable.

## Results
Open the run in GitHub Actions:
- the **job summary** shows the results table (one row per step and per menu, PASS/FAIL, the
  evidence text such as the menu's items, and the screenshot file name);
- the **`ui-evidence-screenshots`** artifact holds the PNGs, `UI_EVIDENCE.md` and the
  `tauri-driver` log.

A failed step does not stop the run; later steps still run and report. The job fails (without
blocking the PR) when any row fails.

## Adding steps
Scenarios are JSON files in `apps/desktop/e2e/scenarios/`; the workflow runs
`scenarios/a1-menus.json` (A1 menus, then the A2 toolbar steps `a2-*`: icon toolbar, grid/unit/snap per editor, split-button picks, Toolbar labels; then the E9 steps `e9-*` (kept for entries still unavailable; Save as and Release check were removed by E1); then the E1 steps `e1-*`: removed placeholders absent, Saved badge, menu-bar arrow keys, status-bar layer per editor, Show <view>, Help and About with Escape, Distributor not configured; then the A3 dock steps `a3-*`: three docks, canvas strip under real pointer moves, a dragged and hidden right dock restored after a webview reload, a part placed from the bottom-dock Library and selected from the design manager). A later UI slice adds steps to a scenario (or a new scenario file plus
one more `node ui-evidence.mjs` line with its own `UI_SCENARIO` and `UI_OUT`) instead of a new
job. `scenarios/e2-editor.json` (slice E2, editor correctness) is such a second scenario: it
runs in its own step with `UI_OUT` under `ui-evidence/e2`, and its rows are added to the job
summary and its screenshots to the same artifact. Its `fx-*` steps (fix place-selbar) check that the schematic canvas does not move when a selection appears and that Place menu picks (Wire, No connect, Net / power label) switch the tool while Place is armed. Its `a5-*` steps (slice A5) check the right dock's inspector: the Label tool's options appear there and not in the editor strip, the selected part's Value edit commits one `SetSymbolValue`, the document shows the sheet size with the title block unavailable, and in the PCB the Route tool's options are in the inspector while the strip keeps the route actions. `scenarios/e3-engine-provenance.json` (slice E3) is a third, under `ui-evidence/e3`. `scenarios/d4-settings.json` (slice D4) is a fourth, under `ui-evidence/d4`: Settings dialog, a refused conflicting rebinding, Save rebound to Ctrl+Alt+S shown in the File menu, toolbar tooltip and palette and still there after a reload, the Canvas colours page found by tree search and the PCB grid page. `scenarios/b1-ir-v3.json` (slice B1, IR schema v3, no UI of its own) is a fifth, under `ui-evidence/b1`: a regression check that a v3 project opens and a placed part is drawn. `scenarios/c2-eco.json` (slice C2, ECO preview) is a sixth, under `ui-evidence/c2`: Update PCB opens the review dialog with the footprint to add; unchecking it disables Apply with the reason; Export report saves the engine's preview; checked again, Apply runs the sync as one Batch and the report lists the added footprint; a second Update PCB finds the PCB already in sync. `scenarios/c3-routing.json` (slice C3, routing options) is a seventh, under `ui-evidence/c3`: with the Route tool active the inspector shows the width and via presets, the obstacle mode (walk around / push / block) with arc corners marked unavailable, loop removal, the ratsnest toggle and the live net length. `scenarios/b2-symbols.json` (slice B2, symbol rendering) is an eighth, under `ui-evidence/b2`: five built-in parts placed through Quick open and one imported KiCad symbol placed from the Library tab, each drawn from its own graphics with pin names and numbers as its definition shows them, then a zoomed-in screenshot. `scenarios/d1-library.json` (slice D1, library dock) is a ninth, under `ui-evidence/d1`: paged built-in grid, category tree, Part / Footprint / 3D kinds and previews, Place on schematic then a click on the sheet (PlaceSymbol), the Project scope listing the placed part, offline mode, the common-parts palette, Standardisation, Manage and the Distributor scope. `e3-engine-provenance.json` switches the Library dock to Manage (D1) before using the part editor, and in `a1-menus.json` the `e1-parts` and `a3-library` steps pick the Distributor and Built-in scopes of the dock. `scenarios/d3-output.json` (slice D3, output documents) runs under `ui-evidence/d3`: a resistor placed with real X input, File > Export BOM with a column removed, a column added, a custom header and a filter rule (the engine preview follows each change), Export CSV showing the DRAFT manifest with the BOM and its saved configuration, the configuration still there after reopening, then File > Export schematic and netlist writing the PDF, SVG, IPC-D-356 and JSON netlist into a DRAFT manifest. `scenarios/d2-editors.json` (slice D2) runs under `ui-evidence/d2`: on a fixture board (outline set by `fixture-command`), the Library dock’s Manage mode › New part… opens the symbol editor, the footprint editor adds a pad and widens the courtyard (AssignComponentFootprint), the symbol editor adds and names a pin (UpsertComponentDefinition), then the part is placed, the PCB updated through the review dialog and auto-placed, DRC shows no errors in implemented checks and the export writes files. `scenarios/b3-editing.json` (slice B3) runs under `ui-evidence/b3`: two parts placed, Ctrl+A, Edit > Align top edges (one `MoveSymbol`: the upper part stays), Ctrl+D duplicate as one `Batch` undone by one Undo, the canvas context menu on a part (registry items, Group unavailable) and on the empty sheet, Ctrl+F find and replace with its match count, and the right dock's selection filter keeping parts out of Ctrl+A. A `click` step takes `"button": "right"` for a right-click. Each step has an `id` (used in screenshot names), a `do` type and an optional `title`.
Every step except `each-menu` takes one screenshot after it runs (set `"shot": false` to skip).

| `do` | Fields | What it does |
|---|---|---|
| `wait` | `selector`, `text`?, `timeout`? (ms) | waits until a visible element matches |
| `fixture-project` | `name`, `folder`? | SETUP: creates the fixture project, reloads to Project Home, waits for it in Recent projects |
| `fixture-library` | `name`, `licence`, `source`, `from`, `part`, `partName` | SETUP (needs an open project): writes an open-source library folder (manifest with licence and source, one definition copied from built-in `from` as `part`) and imports it through `library_import_open_source` |
| `fixture-command` | `command` | SETUP: one typed engine command on the active project through `execute_command` (for example a fixture board outline); run it before the project is opened from Recent projects |
| `fixture-kicad-symbol` | `file`, `lines`, `licence`, `source` | SETUP: writes `lines` as `<file>.kicad_sym` (an original test symbol) and imports it through `kicad_import_commit`, the IPC the KiCad import dialog commits with; fails unless exactly one symbol is imported |
| `click` | `selector`, `text`?, `dx`?, `dy`? (px from the centre), `button`? (`"right"` for a right-click), `expect`?, `expectText`?, `gone`?, `timeout`? | scrolls the element into view if needed (as a user would), XTest click on it (offset by `dx`/`dy`), then waits for `expect` (and for `gone` to disappear) |
| `type` | `selector`?, `text`?, `value`, `expect`?, `expectText`?, `timeout`? | XTest click into the field (without `selector`: the focused field), XTest typing of `value`, then waits for `expect` |
| `menu` | `menu`, `item`, `expect`?, `expectText`?, `gone`? | XTest: opens a top-level menu and clicks the item whose text starts with `item` |
| `key` | `keys` (xdotool key names, space-separated, e.g. `"ctrl+k"`), `expect`?, `expectText`?, `gone`? | XTest key presses |
| `move` | `selector`, `text`?, `at`? (`[fx, fy]` fractions of the element, default centre), `report`?, `changed`? | XTest pointer move; `report`'s text must show a value (and, with `changed`, differ from before the move) |
| `drag` | `selector`, `text`?, `dx`?, `dy`?, `attr`? (default `aria-valuenow`), `save`? | XTest press, move in steps, release; `attr` of the element must change; `save` remembers its new value |
| `check-attr` | `selector`, `attr`, `equalsVar`? | reads an attribute; with `equalsVar` it must equal a value a `drag` saved |
| `gone` | `selector` | waits until no visible element matches |
| `reload` | `expect`?, `expectText`? | reloads the webview (a new UI session with the same app data), then waits for `expect` |
| `absent` | `selector`, `text`?, `timeout`? (ms, default 3000) | passes once no visible element matches (a removed control, a closed dialog); fails if one is still visible |
| `each-menu` | — | opens every top-level menu in turn; one row and one screenshot per menu, listing its items |
| `shot` | — | screenshot only |
| `count` | `selector`, `equals`, `timeout`? | DOM read: waits until exactly `equals` elements match (e.g. markers drawn on the sheet) |
| `rect` | `selector`, `text`?, `save` or `same` | DOM read: `save` stores the element's viewport rect under a name; `same` fails if it moved or resized (more than 0.5 px) since that save (e.g. the canvas when the selection bar appears) |

`text` matches the start of the element's whitespace-normalised text. A click whose target point is covered, clipped or scrolled out (`elementFromPoint` is not the element) fails instead of clicking something else. Prefer the app's
accessible names (`aria-label`) in selectors; they are the same ones the unit tests use.

Example (after `pcb` in `a1-menus.json`):

```json
{ "id": "rules", "do": "menu", "title": "Design rules dialog", "menu": "Design", "item": "Design rules and net classes", "expect": "[role=\"dialog\"]" }
```

## Running elsewhere
Builds and runs happen in GitHub Actions only for this project. The script itself needs Linux,
an X server, `tauri-driver`, `WebKitWebDriver`, `xdotool`, ImageMagick and Node 20+; see the
workflow for the exact steps and environment (`EAK_APP`, `UI_SCENARIO`, `UI_OUT`).
