# Performance verification plan (directive §17, §22 phase 6)

**Status: NOT PERFORMED.** Nothing in this plan has been run. No build, launch, profile, benchmark
or measurement has been done for the performance work (`PERFORMANCE_LEDGER.md`); every change there
is code-reviewed only. Targets below are placeholders to be calibrated from the first real
measurements, never claims.

## 1. Machines

| Tier | CPU | RAM | GPU | Storage | OS / WebView |
|---|---|---|---|---|---|
| Low | 2 cores (or 4 slow threads) | 4–8 GB | integrated | SATA SSD, plus one HDD run | Windows 10/11 WebView2; Ubuntu 22.04 WebKitGTK |
| Balanced | 4–8 cores | 8–16 GB | integrated / entry dGPU | NVMe | Windows 11, Ubuntu 24.04, macOS arm64 WKWebView |
| High | 12+ cores | 32 GB+ | dedicated | NVMe | Windows 11, Ubuntu 24.04 |

Run each tier with Performance mode on AUTO, then forced LOW and HIGH, and check that the profile
(`resource_profile` IPC; Settings → Performance) matches the machine and changes the limits.

## 2. Projects
Small (fixture), medium (~200 parts, 2 layers), large (~1 500 parts, 4 layers, zones, 3 000+ tracks).
Large projects are generated through `eak` CLI commands (no hand-edited project files).

## 3. Workflows and what to record

| # | Workflow | Metrics | Expected observation (to confirm) | Ledger |
|---|---|---|---|---|
| 1–2 | Cold / warm start to Project Home | time to first paint, to interactive | optional subsystems do not delay Home | — |
| 3–4 | Open / switch project | open time; memory retained after switch | shared snapshot released on close | L1 |
| 5–6 | Large schematic open; move a symbol | render commit time, P50/P95 input latency | at most one render per frame during drag | L4, L6 |
| 7–8 | PCB edit; move 50 footprints | frame-time distribution; `execute_command` count | one commit on release | L4 |
| 9 | Manual routing while DRC runs | `route_step` latency P95 | preview not blocked by DRC | L1, L2 |
| 10 | Switch layers | frame time | — | L16 |
| 11–13 | 3D open, orbit a large board, load/unload models; force `WEBGL_lose_context` | frame time, idle GPU %, live contexts after 30 open/close cycles | no frames while idle; message on loss; redraw on restore | L7–L9 |
| 14–15 | Large library; search typing | keystroke-to-results | not blocked by checks | L17, L19 |
| 16–17 | Terminal input/scroll; 8 busy sessions (`yes`, build logs) | echo latency, memory per tab, event rate | view scrollback bounded by the profile | L14, L15 |
| 18 | 4 concurrent DRC/DFM + an export | queue wait, `execute_command` latency, CPU % | checks queue at `analysis_jobs`; edits unaffected | L2, L3 |
| 19 | Save under rapid edits | `execute` + atomic write time vs project size | measure write amplification | L10, L11 |
| 20 | EDA ↔ Terminal workspace switching | switch time | — | — |

Always record P50, P95 and max: a low average with multi-second stalls fails.

## 4. Tools
- Rust: `samply`, or `perf` with `hotspot`/`flamegraph`, on the desktop binary; `tokio-console`
  (needs a `console-subscriber` dev build) for blocking-pool and semaphore wait times.
- WebView: Chrome DevTools Performance (WebView2), Safari Web Inspector (WKWebView), the WebKitGTK
  inspector; React Profiler for commit counts; three.js `renderer.info` for draw calls and memory.
- OS: Task Manager / `top` / Activity Monitor for idle CPU and RSS.

## 5. Correctness regression checks (must not change with the performance mode)
- ERC/DRC/DFM/MECH reports identical for the same revision under LOW and HIGH (diff the evidence JSON).
- A check started at revision N while an edit commits: the check returns the "project changed"
  error and writes no evidence for N (L3).
- Gerber/drill/BOM/CPL byte-identical across modes for the same revision.
- No UNKNOWN or NOT_RUN gate reported as PASS in any mode.
- Unit tests written for this work and never run: `apps/desktop/src-tauri/src/resources.rs` (tests
  module), `apps/desktop/src/canvasFrame.test.tsx`.

## 6. Pass/fail
Thresholds are set after the first baseline run per tier, then tracked as regressions (for example
P95 above baseline × 1.2 fails). Until then results are reported as measured values only.
