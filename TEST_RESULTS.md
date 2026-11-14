# Test results

Record of test runs for the EDA desktop directive (2026-11-14 onward), newest first. Each entry says
what was run, where, on which commit, and the exact outcome. A result copied from somewhere else says
so. Level 1 = Rust engineering tests, Level 2 = frontend tests (vitest + jsdom), Level 3 = real desktop
tests in a Tauri WebView (directive Part 13).

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
