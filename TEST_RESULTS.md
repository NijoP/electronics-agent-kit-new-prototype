# Test results

Record of test runs for the EDA desktop directive (2026-11-14 onward), newest first. Each entry says
what was run, where, on which commit, and the exact outcome. A result copied from somewhere else says
so. Level 1 = Rust engineering tests, Level 2 = frontend tests (vitest + jsdom), Level 3 = real desktop
tests in a Tauri WebView (directive Part 13).

## 2026-11-15 - Full local batch run on v0.1.3 (`2153054`)

**Run:** `scripts/test-batches.sh --lock-once` on `origin/main` `2153054` (v0.1.3), Linux, 4 cores / 7.8 GB RAM /
12 GB swap, shared with other jobs. Private worktree `target/`, `CARGO_BUILD_JOBS=2`, one flock on the host
build lock (no wait). Monitor: `scripts/eak-res.sh`, 1 s sampling. Result: **12 of 12 batches passed, 0
failed.** Rust: 236 passed, 0 failed (sum of every cargo `test result` line, doctests included).
Frontend: vitest 122 tests passed (an earlier run on the older base counted 65). Not run: `package:tauri`
(opt-in `--package`), so packaging was not verified by this run.

| Batch | Result | Duration | Peak tree RSS | Min MemAvailable | Peak swap used (host) |
|---|---|---|---|---|---|
| light:fmt | PASS | 9.6 s | 46 MB | 3530 MB | 3488 MB |
| light:no-python | PASS | 1.2 s | 0 MB (ended between samples) | 3543 MB | 3488 MB |
| light:tsc | PASS | 77.1 s | 383 MB | 2526 MB | 3488 MB |
| light:vitest | PASS | 191.5 s | 581 MB | 1994 MB | 3471 MB |
| test:eak-core | PASS | 161.0 s | 624 MB | 2722 MB | 3471 MB |
| test:eak-engineering | PASS | 108.6 s | 559 MB | 2754 MB | 3463 MB |
| test:eak-validation | PASS | 386.5 s | 631 MB | 2619 MB | 3466 MB |
| test:eak-manufacturing | PASS | 536.2 s | 760 MB | 2075 MB | 3477 MB |
| test:eak-intelligence | PASS | 259.5 s | 730 MB | 2511 MB | 3500 MB |
| test:eak-runtime (`--test-threads=2`) | PASS | 250.1 s | 558 MB | 2338 MB | 3238 MB |
| test:eak-desktop | PASS | 1258.4 s | 1269 MB | 2107 MB | 3962 MB |
| clippy `--workspace --all-targets -D warnings` | PASS | 244.3 s | 1437 MB | 3598 MB | 3501 MB |

Peak tree RSS is the command's own process tree; swap and MemAvailable are whole-host and include other
jobs. The largest single batch peak was 1437 MB (clippy), well under the 2 GB / 2.5 GB figures that
originally triggered this work, but those figures were for the LCSC tests and packaging, and packaging was
not run here. An earlier attempt on `c1709ff` used a shared target dir that other worktrees also wrote to;
it was discarded and nothing from it is reported as a result.

## 2026-11-14 - Low-memory tooling, light batches (PARTIAL)

**Run:** `scripts/test-batches.sh --no-lock --only light` on a branch from `ea1efeb`, Linux, 8 GB host
shared with other jobs. Full numbers and method: `docs/dev/LOW_MEMORY_DEVELOPMENT.md`.

| Batch | Result | Duration | Peak tree RSS |
|---|---|---|---|
| light:fmt | PASS | 7.4 s | 45 MB |
| light:no-python | PASS | 1.2 s | - |
| light:tsc | PASS | 52.1 s | 367 MB |
| light:vitest | PASS, 10 files / 65 tests | 118.2 s | 552 MB |

**Not run yet:** per-crate Rust tests (all 7 crates), clippy, packaging. They are pending a run on the
frozen main commit; nothing here claims they pass.

**Note (evaluator finding C6):** results for PRs #21-#26 (shell, multilayer, menus/tabs, installers,
batch command) were recorded only in `STATUS.md` (for example vitest 52 then 65+ tests, build and tsc
OK, production bundle checked in headless Chrome against a stubbed IPC, never launched as a real Tauri
window). They are not re-run or copied here; see the `STATUS.md` sections of those PRs. `FEATURE_MATRIX.md`
was corrected from the code: DRC declares 17 rules (`DRC-001`..`DRC-017`), ERC 13, MECH 3, and there is
no MECH toolbar button.

## 2026-11-14 - Release packaging dry run (v0.1.0, commit ab482da)

**Where:** Zorin OS 18.1 (Ubuntu 24.04 base, glibc 2.39), x86_64, 8 GB RAM; rustc 1.97.1, Node 22.23.2.
`ab482da` is `main` at `ea1efeb` plus the packaging branch.

| Check | Result |
|---|---|
| `bash scripts/release-build.sh` (`npm ci`, `tauri build --bundles deb,appimage`) | exit 0; `EAK_0.1.0_amd64.deb` (6,223,550 bytes) and `EAK_0.1.0_amd64.AppImage` (87,333,368 bytes) + `SHA256SUMS`. AppImage plugin download failed; Tauri's built-in AppImage tooling was used |
| `.deb` extracted (`dpkg-deb -x`), binary launched on Xwayland | up after 12 s, window "EAK" 1400x900 mapped, recent-project index created, no log errors, exited on `kill` |
| AppImage launched directly | up after 15 s with bundled WebKitGTK processes, window capture shows Project Home, exited on `kill` |
| `npx vitest run` (Level 2) | 10 files, **65 passed, 0 failed** |
| Rust tests | not run for packaging |
| Level 3 | launch check only, no interaction |

## 2026-11-14 - Phase 0 baseline (copied, not re-run)

**Source:** the merged pull request bodies of #12-#19
(`gh pr view <n> --repo NijoP/electronics-agent-kit-new-prototype`) and `STATUS.md` at `afd9351`.
**Not re-run** for Phase 0: this phase is documentation only and four other workers share the 8 GB host.

| PR | Merge commit | Level 1 (Rust) | Level 2 (vitest) | Level 3 | Platform |
|---|---|---|---|---|---|
| [#19](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/19) | `afd9351` | fmt, clippy OK; `cargo test -p eak-runtime --release` all passed; full `cargo test --workspace --release`: **1 failure**, `eak-runtime/tests/process.rs::captures_output_and_exit_code` (timing race suspected, not confirmed or fixed) | not run | none | Linux |
| [#18](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/18) | `255236a` | fmt, clippy OK; `cargo test --workspace --release --exclude eak-runtime`: **113 passed, 0 failed** (eak-runtime excluded under the kill_tree safety rule of the time) | **43 passed** | none | Linux |
| [#16](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/16) | `5152fab` | fmt, clippy OK; workspace tests all passed (eak-intelligence 25 unit + 4 loopback HTTP + 13 agent; eak-desktop ipc 8 + models 9); real OS keyring round trip passed | 23 passed | none | Linux |
| [#15](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/15) | `c1c4635` | fmt, clippy OK; workspace tests all passed (eak-desktop ipc 11) | 33 passed | none | Linux |
| [#14](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/14) | `1101640` | fmt, clippy OK; `cargo test --workspace --release -- --test-threads=2`: **58 passed, 0 failed**; one earlier run had the same `process.rs` failure | not run | none | Linux |
| [#13](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/13) | `4866887` | docs only, no run recorded | - | none | - |
| [#12](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/12) | `75e13b0` | `cargo test --workspace --release`: **43 passed**; CI 8 jobs green on commit 599bf91 (Windows, Ubuntu, macOS arm64, macOS x64) | 18 passed; `tauri build --no-bundle` OK; 6-second launch smoke check | none | Windows 11 local + CI |

**Known issues carried forward**

- `eak-runtime/tests/process.rs::captures_output_and_exit_code` is intermittent under load (PR #14,
  PR #19). Unresolved.
- GitHub Actions has been disabled (billing) since PR #13; Windows and macOS have not been verified
  since commit 599bf91.
- No Level 3 (real desktop) test exists.

**Phase 0 verification of this documentation PR:** `scripts/check-no-python.sh` and a Markdown
link/format review; results are recorded in the pull request body.
