# Test results

Record of test runs for the EDA desktop directive (2026-11-14 onward), newest first. Each entry says
what was run, where, on which commit, and the exact outcome. A result copied from somewhere else says
so. Level 1 = Rust engineering tests, Level 2 = frontend tests (vitest + jsdom), Level 3 = real desktop
tests in a Tauri WebView (directive Part 13).

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
