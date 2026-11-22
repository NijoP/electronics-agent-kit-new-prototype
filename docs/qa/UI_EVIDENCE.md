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
`scenarios/a1-menus.json` (A1 menus, then the A2 toolbar steps `a2-*`: icon toolbar, grid/unit/snap per editor, split-button picks, Toolbar labels; then the E9 steps `e9-*`: menu entries that are explicitly unavailable show their reason). A later UI slice adds steps to a scenario (or a new scenario file plus
one more `node ui-evidence.mjs` line with its own `UI_SCENARIO` and `UI_OUT`) instead of a new
job. `scenarios/e2-editor.json` (slice E2, editor correctness) is such a second scenario: it
runs in its own step with `UI_OUT` under `ui-evidence/e2`, and its rows are added to the job
summary and its screenshots to the same artifact. `scenarios/e3-engine-provenance.json` (slice E3) is a third, under `ui-evidence/e3`. Each step has an `id` (used in screenshot names), a `do` type and an optional `title`.
Every step except `each-menu` takes one screenshot after it runs (set `"shot": false` to skip).

| `do` | Fields | What it does |
|---|---|---|
| `wait` | `selector`, `text`?, `timeout`? (ms) | waits until a visible element matches |
| `fixture-project` | `name`, `folder`? | SETUP: creates the fixture project, reloads to Project Home, waits for it in Recent projects |
| `fixture-library` | `name`, `licence`, `source`, `from`, `part`, `partName` | SETUP (needs an open project): writes an open-source library folder (manifest with licence and source, one definition copied from built-in `from` as `part`) and imports it through `library_import_open_source` |
| `click` | `selector`, `text`?, `dx`?, `dy`? (px from the centre), `expect`?, `expectText`?, `timeout`? | XTest click on the element (offset by `dx`/`dy`), then waits for `expect` |
| `type` | `selector`, `text`?, `value`, `expect`?, `expectText`?, `timeout`? | XTest click into the field, XTest typing of `value`, then waits for `expect` |
| `menu` | `menu`, `item`, `expect`?, `expectText`? | XTest: opens a top-level menu and clicks the item whose text starts with `item` |
| `key` | `keys` (xdotool key names, space-separated, e.g. `"ctrl+k"`) | XTest key presses |
| `each-menu` | — | opens every top-level menu in turn; one row and one screenshot per menu, listing its items |
| `shot` | — | screenshot only |
| `count` | `selector`, `equals`, `timeout`? | DOM read: waits until exactly `equals` elements match (e.g. markers drawn on the sheet) |

`text` matches the start of the element's whitespace-normalised text. Prefer the app's
accessible names (`aria-label`) in selectors; they are the same ones the unit tests use.

Example (after `pcb` in `a1-menus.json`):

```json
{ "id": "rules", "do": "menu", "title": "Design rules dialog", "menu": "Design", "item": "Design rules and net classes", "expect": "[role=\"dialog\"]" }
```

## Running elsewhere
Builds and runs happen in GitHub Actions only for this project. The script itself needs Linux,
an X server, `tauri-driver`, `WebKitWebDriver`, `xdotool`, ImageMagick and Node 20+; see the
workflow for the exact steps and environment (`EAK_APP`, `UI_SCENARIO`, `UI_OUT`).
