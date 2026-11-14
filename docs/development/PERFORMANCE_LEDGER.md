# Performance baseline map and bottleneck ledger

Source inspection only (directive §22 phases 1–2, 2026-11-15). Nothing was built, launched,
profiled or measured. "Confirmed" means the cost is visible in the source; it does **not** mean it
was observed to be slow. "Risk" means it may matter at scale and needs measuring first.

## 1. Baseline map (phase 1)

| Area | How it works today (evidence) |
|---|---|
| Startup | `apps/desktop/src-tauri/src/lib.rs` `run()` setup: opens the SQLite recent index, starts the harness endpoint, terminal manager + control API, model settings. Each optional part logs and continues. Frontend `main.tsx` → `App.tsx` → Project Home; editors mount on open. |
| Rust command layer | ~68 `#[tauri::command]`s. All project/library/model commands are sync fns marked `async`, so Tauri runs them as tasks on its Tokio runtime (`lib.rs` `mod cmd`). Every project command takes the one `Session.engine: Mutex<Open>` (`lib.rs` `Session::with_engine`). |
| Snapshot / delta | `Engine::execute` applies a command to a clone, diffs sections into a delta, journals, atomically rewrites `project.eak.json` (ADR-0001/0003). The UI never patches: after each edit it calls `current_project`, which clones the whole `Project` and recomputes `geometry()` (`lib.rs` `snapshot`, `Workspace.tsx` `setSnapshot(await api.currentProject())`). |
| Frontend state | `Workspace.tsx` owns snapshot, selection, results, layout in ~30 `useState`s; editors receive the snapshot as props. No external store. |
| Schematic / PCB | SVG in `Canvas.tsx` (viewBox pan/zoom); `SchematicView.tsx`, `PcbView.tsx` render every object each render. Drag previews are local state; one command (or `Batch`) on pointer-up. |
| Interactive routing | `PcbView.tsx` hover → 30 ms debounce → `route_step` IPC with a sequence number that drops stale replies. Backend computed against the live engine under the engine lock. |
| 3D | `viewer3d/Viewer3D.tsx`: one `WebGLRenderer`, already demand-driven (render on controls `change`, resize, rebuild, layer toggle; rAF-coalesced; no permanent loop). Geometry instanced per layer; groups disposed on rebuild/unmount. Snapshot re-read only while the view is active. |
| Storage | Per command: journal pending (fsync) → atomic full-file rewrite → journal committed (`crates/eak-runtime/src/store.rs`). No autosave timer; every commit is durable. |
| Terminal | `crates/eak-terminal`: one reader + one reaper thread per PTY, 16 KiB reads, each chunk emitted as a Tauri event; backend scrollback ring 2 MiB per session (`SCROLLBACK_BYTES`). xterm.js kept 10 000 lines (`terminal/XTermView.tsx`). |
| Background work | No scheduler or job queue. Harness endpoint: one thread per connection (`crates/eak-harness/src/endpoint.rs`). Event poll every 1.5 s in `Workspace.tsx`; terminal status poll 1.5 s while shown; overview poll 4 s while shown. |
| Validation | ERC/DRC/DFM ran synchronously in `run_check` on the live project under the engine lock, then saved as evidence. |
| Caching | None in the desktop layer (each read command recomputed from the live project). |

## 2. Bottleneck ledger (phase 2)

Severity is for user-visible responsiveness. Status: **fixed** (code-reviewed only, not
runtime-verified), **open**, or **by design** (needs an ADR to change).

| # | Issue | Evidence | Impact | Cause | Sev | Kind | Proposed fix | Risk | Verify later | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| L1 | Long CPU work holds the single engine lock | `lib.rs` `run_check`, `connectivity`, `domain_classification`, `route_step`, `library_search`, `export_manufacturing` ran inside `with_engine` | A DRC or export freezes drag commits, undo and route previews until it ends | Read-only analysis runs on `&Engine` under the `Mutex` | High | Confirmed | Take an `Arc<Project>` of the current revision under the lock (cloned once per revision), compute without the lock | One extra project copy per revision | P95 `execute_command` latency while DRC runs on a large board | fixed (code-reviewed only) |
| L2 | Blocking work on Tokio worker threads, unbounded | Sync fns with `#[tauri::command(async)]` execute on the async runtime | Concurrent checks/exports/simulations can occupy every worker on a 2–4 core machine; all other IPC waits | No `spawn_blocking`, no limit | High | Confirmed | Heavy commands become `async fn` → `spawn_blocking` behind a semaphore sized by the resource profile | Edits are not routed through the pool, so their order is unchanged | IPC latency with 4 parallel DRCs | fixed (code-reviewed only) |
| L3 | Stale check results | Evidence is bound to the revision read; UI marks reports stale by revision | Once L1 moves work off the lock, a report may finish after a newer edit | Inherent once work leaves the lock | Med | Confirmed (after L1) | Report keeps its revision; evidence saved only if the engine is still on it, otherwise the call fails "project changed, re-run" | A check on a busy project needs a re-run | Edit during DRC → no evidence for the old revision | fixed (code-reviewed only) |
| L4 | Pointer moves re-render the whole editor per event | `Canvas.tsx` `onPointerMove` calls `onHover`/`onDrag.move`/`setVb` per DOM event | Drag/pan/hover on large boards: more renders than frames | No per-frame coalescing | High | Confirmed | First move of a frame handled at once, later ones in that frame folded into one rAF; flush on pointer-up | None to precision (last position always applied) | Frame times while dragging 500 footprints | fixed (code-reviewed only) |
| L5 | Whole snapshot over IPC after every edit | `Workspace.tsx` `setSnapshot(await api.currentProject())`; `lib.rs` `snapshot()` clones `Project` + geometry | Large projects: serialize + parse per edit | No typed delta to the UI | High | Confirmed | Typed incremental notifications (project id, revision, changed entities) | Shared contract → ADR | IPC payload size/time per edit | open (ADR needed) |
| L6 | Selection change re-renders every editor and panel | `Workspace.tsx` `selected` at the top; views get inline callbacks | Large schematic: selection feels heavy | State scoped too high; unstable props defeat memo | Med | Confirmed | Selection in a small store with selective subscription | Cross-probe behaviour | React profiler commit time on select | open |
| L7 | No WebGL context-loss handling | `Viewer3D.tsx` had no `webglcontextlost` listener | GPU reset leaves a dead canvas | Not handled | Med | Confirmed | Listen for lost/restored, show a message, rebuild on restore; editors unaffected | None | Force `WEBGL_lose_context` | fixed (code-reviewed only) |
| L8 | WebGL context held after unmount | `Viewer3D.tsx` cleanup called `renderer.dispose()` only | Repeated open/close can exhaust the WebView's context limit | No `forceContextLoss()` | Low | Confirmed | `forceContextLoss()` after `dispose()` | None | Open/close 3D 30× | fixed (code-reviewed only) |
| L9 | Fixed 3D quality | `Viewer3D.tsx` `antialias: true`, pixel ratio ≤ 2 | Integrated GPUs pay ~4× fill at HiDPI | No tier | Med | Confirmed | Pixel-ratio cap and antialias from the resource profile | Visual only; geometry unchanged | GPU frame time per tier | fixed (code-reviewed only) |
| L10 | Full-file rewrite per command | `eak-runtime/src/store.rs` atomic write each commit | Large project + rapid edits: write amplification | ADR-0001 single canonical file | Med | Confirmed | Measure first; options via ADR (journal compaction) | Durability | Save latency vs size | open (by design) |
| L11 | Clone of whole project per command | `Engine::execute` applies on a clone (ADR-0003) | Per-edit cost grows with size | Transactional safety design | Med | Confirmed | Measure first | Integrity | `execute` time vs size | open (by design) |
| L12 | Harness endpoint: thread per connection, unbounded | `eak-harness/src/endpoint.rs` `thread::spawn(serve_conn)` | Misbehaving agent can create many threads | No limit | Low | Confirmed | Counter-bounded accept | Agent sees "busy" | Connection flood | open |
| L13 | Event poll runs while the window is hidden | `Workspace.tsx` `setInterval(pullEvents, 1500)` | Idle CPU/IPC in background | No visibility check | Low | Confirmed | Skip ticks while `document.hidden` | Agent edits appear on the next visible tick | Idle CPU minimised | fixed (code-reviewed only) |
| L14 | xterm scrollback fixed at 10 000 lines per tab | `terminal/XTermView.tsx` | Many tabs on 4 GB machines | Not tied to a budget | Low | Confirmed | Lines from the resource profile; backend 2 MiB ring unchanged (documented retention) | Fewer lines in view on LOW | Memory with 8 busy tabs | fixed (code-reviewed only) |
| L15 | Every PTY chunk is a separate Tauri event | `eak-terminal/src/lib.rs` pump: one `sink` call per read | Chatty programs: many small events | No batching | Low | Risk | Coalesce reads for a few ms | Echo latency | Event rate under `yes` | open |
| L16 | SVG editors render every object, no culling | `SchematicView.tsx`, `PcbView.tsx` map all objects | Thousands of objects: slow render | No viewport culling | Med | Risk | Cull by view box; spatial index for hit tests if needed | Selection identity | Render time vs count | open |
| L17 | Library search clones the project library under the lock | `lib.rs` `library_search` `e.project().library.clone()` | Typing in search | Clone under lock | Low | Confirmed | Use the per-revision `Arc<Project>` (L1) | None | Keystroke-to-result | fixed (code-reviewed only) |
| L18 | No central resource profile | No module decides concurrency/cache/3D tiers | Subsystems size themselves independently | Missing | Med | Confirmed | `resources.rs` ResourceProfile + persisted preference | None | Profile matches machine | fixed (code-reviewed only) |

## 3. Not in scope here
3D model loading/quality (3D-components lane) and validation-state semantics (validation lane).
No fix changes an engineering value, geometry or validation outcome; UNKNOWN never becomes PASS.
