# Performance observations (installed f48158d, disposable instance)

Method: `/proc/<pid>` of the app and its descendants (RSS from `VmRSS`; CPU from utime+stime deltas over 2-5 s windows on 4 logical CPUs, 100 % = one core), AT-SPI polling for latencies (adds tens of ms), `XResizeWindow` for resize. Single run each: observations, not benchmarks. Raw log is outside the repo.

| State | Processes | RSS MB | Threads | CPU % |
|---|---:|---:|---:|---:|
| Idle, Project Home, fresh start | 3 (app, WebKitWebProcess, WebKitNetworkProcess) | 407.7 | 69 | 0.2 |
| Terminal workspace, 1 bash open (first run) | 4 | 525.7 | 66 | 5.6 |
| Terminal workspace, 3 panes / 4 sessions (bash, claude stub, pi stub) | 10 | 475.4 | 82 | 6.5 |
| Same, after switching/closing, quiet | 10 | 476.5-479.9 | 78 | 0.5-1.0 |
| After closing one session | 9 | 473.5 | 79 | 4.6 (transient) |

- Terminal process count follows sessions; closing a session removed its process (10 to 9), no zombie.
- RSS did not fall after closing (WebKit retains memory); it rose from ~410 to ~525 MB when a terminal was first opened.
- A visible terminal costs ~5-6 % of one core (probably cursor blink/repaint; not profiled).
- Startup: window mapped ~6.6 s, accessible UI ~7.2 s after launch (polling overhead included, so an upper bound).
- Workspace switch: Terminal to EDA 5.4 s right after a resize, EDA to Terminal 1.4 s; project open/recover 8-16 s in two attempts (includes the two-step recovery confirm and my polling). Noisy and unexplained; not product figures.
- Window resize 1400x900 to 1000x640: content reflowed in ~0.75 s (includes the window manager).
- Window 1400x900 on a 1920x1080 screen, scale 1.0. Terminal viewport 1180x360 px = 24 rows x 149 cols = 44.6 % of the 808 px editor height; width 100 %.
- Not measured: GPU, per-process CPU split, long-session memory growth, input latency, battery. No stress tests run.

# Canvas2D PCB renderer benchmark (slice A4, 2026-11-23)

Target (plan slice A4): at least 30 fps pan and zoom with 2,000 footprints and 20,000 track
segments (59,250 render objects) on the 8 GB reference host.

**Status: target not met on the CI runner; the reference-host run is pending.** Builds and
benchmarks are banned on the development laptop (the 8 GB reference host), so the only
measurements so far come from the CI job `render-qa`. That job runs Chrome 157 under Xvfb with
software rasterisation (no GPU) on a GitHub-hosted ubuntu runner with 4 vCPUs (Xeon) and 15 GB.
The canvas is 1600x1000 CSS px at devicePixelRatio 1. Frame rate is measured with
requestAnimationFrame over 3 s per scenario; render ms is JavaScript time per frame. The
benchmark is `src/render/qa/main.tsx` `bench()`; the board is generated (`benchmarkBoard()`,
seed 42).

| Scenario | Run 45063731096 fps | Run 45064284653 fps | Mean render ms (2nd run) | Full layer redraws/frame |
|---|---:|---:|---:|---:|
| Pan, whole board in view (2 px steps) | 31.0 | 25.5 | 35.3 | 0 (cache shift + strips) |
| Pan, editing zoom 20 px/mm (2 px steps) | 32.2 | 26.4 | 33.9 | 0.1 |
| Zoom around 20 px/mm | 31.2 | 27.4 | 32.2 | 9.9 |
| Zoom, whole board in view | 10.3 | 11.5 | 82.3 | 9.7 |
| Pan, whole board, sub-pixel steps (no cache reuse) | 15.1 | 16.8 | 55.3 | 10.0 |

Other figures from the second run:
- scene build 75 ms, `setScene` 36 ms, first full render 100 ms;
- a pick takes 14.8 µs;
- a one-track edit rebuilds the scene in 68 ms and redraws in 98 ms (1 dirty rectangle, no full
  layer redraw).

Shortfall, stated plainly:
- **Zooming with the whole board in view runs at 10-12 fps**, 18-20 fps short. Every zoom step
  redraws every layer, and at that zoom all 59,250 objects are on screen.
- **Sub-pixel pans run at 15-17 fps**, 13-15 fps short. They cannot reuse cached pixels.
- **Whole-pixel pans and zoom at editing scale sit at the target**: 25-32 fps across the two
  runners, about 5 fps apart from run to run.

Candidate next steps (not done in A4):
- during a zoom gesture, show the scaled cached layers and redraw once the gesture settles;
- snap pans to whole device pixels;
- level of detail at whole-board zoom (skip pads and labels under a pixel);
- a WebGL backend behind the same `SceneRenderer` interface.

Not measured: the Tauri WebKitGTK webview, the 8 GB reference host, devicePixelRatio 2, memory
use of the layer caches (budget 96 MiB).

Known limitation: the jsdom unit tests do not prove that the canvas paints. In jsdom with a stub
2D context, the scene canvas set its painted-frame flag (`data-draw="drawn"`,
`data-frames="1"`) while a spy on `Canvas2DRenderer.render` saw 0 calls and the stub context
recorded 0 calls. This is unexplained after six CI diagnostic rounds (2026-11-23). The jsdom
tests keep what they can prove: scene and pick-table construction, renderer selection, the SVG
fallback flag and picking parity. Real painting is asserted by the browser job `render-qa`:
pixel diffs and real-click picking.
