# Desktop end-to-end workflow test

A real-window test of the EAK desktop app: [tauri-driver](https://crates.io/crates/tauri-driver)
+ WebdriverIO drive a **debug build** of `apps/desktop` (Tauri 2 / WebKitGTK). Clicks go to the
real UI, commands run in the real Rust engine, and results are checked against the engine
snapshot and the files on disk. Nothing is mocked.

Workflow covered (one step each, `S01`–`S18`): launch → create project → schematic → place two
parts → connect pins → connectivity → sync to PCB → board outline → auto-place footprints →
4 copper layers → auto-route → copper zone → ERC → DRC → export outputs (Gerber/drill/BOM/CPL,
plus the ERC/DRC/MECH reports export runs) → save → close → reopen from Recent and compare.

A step with no UI control is driven through the same Tauri IPC command the UI uses
(`window.__TAURI_INTERNALS__.invoke`) and is labelled `IPC (no UI)` in the results, so the
missing UI is visible rather than hidden. A failing step does not stop the run.

## Requirements (Linux)

- `tauri-driver`: `cargo install tauri-driver --locked`
- `WebKitWebDriver` (Debian/Ubuntu package `webkit2gtk-driver`)
- An X display: a real one, or `Xvfb` (package `xvfb`)
- Node 20+

Windows (`msedgedriver`) is supported by tauri-driver but not exercised here yet; macOS has no
WKWebView driver, so this test cannot run there.

## Run

```sh
# 1. debug build with the frontend embedded (no dev server needed)
cd apps/desktop && npm ci && npx tauri build --debug --no-bundle
# 2. test dependencies
cd e2e && npm ci
# 3. run (Xvfb shown; drop xvfb-run to use the real display)
EAK_APP=$CARGO_TARGET_DIR/debug/eak-desktop \
E2E_OUT=/tmp/eak-e2e \
  xvfb-run -a -s "-screen 0 1600x1000x24" node run.mjs
```

Environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `EAK_APP` | `$CARGO_TARGET_DIR/debug/eak-desktop` | app binary under test |
| `TAURI_DRIVER` | `tauri-driver` | tauri-driver binary |
| `NATIVE_DRIVER` | from `PATH` | `WebKitWebDriver` binary |
| `E2E_OUT` | new temp dir | project folder, app data, logs |
| `E2E_SHOTS` | `$E2E_OUT/screenshots` | one PNG per step |
| `E2E_RESULTS` | `$E2E_OUT/E2E_RESULTS.md` | markdown results table |

The app runs with `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_CACHE_HOME` under `$E2E_OUT/app-data`,
so the recent-project index and saved layout start empty and never touch your own profile.
Exit code is 0 only when every step passes. Latest recorded run: `docs/qa/E2E_RESULTS.md`.

## Render QA (Canvas2D PCB renderer, slice A4)

`render-qa.mjs` is a separate, dependency-free check used by the CI job `render-qa`. It starts the
Vite dev server, opens `render-qa.html` (dev-only; not in the packaged build) in a real Chrome
window under Xvfb, and drives it over the Chrome DevTools Protocol:

- pixel-diff snapshots of the three reference boards (`src/render/fixtures.ts`) against
  `render-baselines/*.png`. A pixel counts as changed when a channel differs by more than 48; a
  board fails above 0.2 % changed pixels or when its baseline is missing;
- the PCB editor on a reference board with each renderer: which renderer is in use, and whether
  a real mouse click on R2's pad selects R2;
- screenshots of the editor, including the 2,000-footprint benchmark board;
- the 2,000-footprint / 20,000-segment pan and zoom benchmark. It is recorded and never fails the
  job: a result below 30 fps is reported as a shortfall.

```sh
xvfb-run -a node e2e/render-qa.mjs            # needs npm ci and Chrome/Chromium (or CHROME=...)
RENDER_QA_UPDATE=1 xvfb-run -a node e2e/render-qa.mjs   # rewrite baselines; review before committing
```

Output goes to `RENDER_QA_OUT` (`RENDER_QA.md`, `render-qa.json`, `snapshots/`, `diffs/`,
`screens/`). In CI it is uploaded as the `render-qa` artifact and shown in the job summary.
Baselines depend on the browser's rasteriser. After a Chrome or runner-image update, review the
diff images and refresh the baselines.
