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
