# EAK v0.2.0-rc.1: release status

**Kind:** an unsigned pre-release. The tag is `v0.2.0-rc.1` (the app version is 0.2.0, from #112). It is built by
`.github/workflows/release.yml` from one commit that the `ci` workflow has already proved green.

**Not a manufacturing-ready claim.** EAK never calls a design manufacturing-ready. The release gates and their limits are
shown in the app (Manufacturing › Release check).

**Living document:** this page is updated while PRs land, up to the code freeze (17:30 IST, 2026-11-26). The row for any PR
merged after this page was last edited reads "not in this release" until the page is updated.

## Status labels

| Label | Meaning |
|---|---|
| **landed** | Merged to main and verified by the CI job or ui-evidence scenario named in the row. |
| **partial** | Merged, but only part of the feature is reachable: usually an engine or agent capability with no UI yet. The row says what is missing. |
| **unavailable** | Not merged. The app does not offer it, or shows it as unavailable; it is never faked. |

"ui-evidence" means `.github/workflows/ui-evidence.yml`: the real app is driven under Xvfb on Linux, one
`apps/desktop/e2e/scenarios/*.json` file per step. "ci" means `.github/workflows/ci.yml`: fmt, clippy `-D warnings`,
cargo test and vitest on Linux, plus builds and tests on Windows, macOS arm64 and macOS x64.

## Features since v0.1.5-rc.2

| Feature | Status | PR / commit | Verified by |
|---|---|---|---|
| v2 design tokens (TUSK colours, materials kept apart from the accent) and shell chrome: top bar, navigation rail (Home, Schematic, PCB, 3D, Library, Agents, Checks, Manufacturing, Terminal), explorer panel, status bar | landed | #110 | ci: vitest `design/tokens.test.ts`, `shell/shellFlow.test.tsx`, `render/pcbObjects.test.ts`; every ui-evidence scenario, each of which drives the app through the rail |
| Project Home: project facts, Needs attention, design brief, first-run tour | landed | #108 | ui-evidence `g1-home-firstrun.json`; vitest `home.s1.test.tsx`, `home/homeFacts.test.ts`; Rust `src-tauri/tests/home.rs` |
| One board geometry for DRC, connectivity, 2D and 3D (E1) | landed | #107 | ci: Rust `board.rs` unit tests, `eak-validation/tests/board_geometry.rs` |
| Real-window PCB workflow scenario (create a project, draw by hand, route, auto-route) | landed | #99 | ui-evidence `p1-pcb-flow.json` |
| Definition-of-done fixes: F3 edit integrity, F4 ui-evidence and inventory gates, F5 drawing tools, F7 navigation and cross-probe, F9 library and panels honesty, F10 rules and inspector reasons, F11 docs truth pass | landed | #97, #93, #96, bf88f87, dd6e42d, b43b5e6, 11bd4bc | ui-evidence `f3-edit-integrity.json` and the scenarios cited in STATUS.md rows F7, F9 and F10; vitest; `scripts/check-status-evidence.sh` |
| Agent propose mode: proposals are checked on a copy and applied only by a person | partial: the agent side (`eak --propose`, MCP `proposal_*` tools) works; the in-app review dialog (#111) has not landed | #106 | ci: Rust e2e `crates/eak-harness/tests/proposal.rs` |
| WEFT agent-to-agent orchestration | partial: works over the CLI and `eak-mcp`; the in-app activity view (#126) has not landed | #117 | ci: Rust e2e `crates/eak-harness/tests/weft.rs` (two real `eak-mcp` clients) |
| Footprint verification FP-001..FP-010 | partial: an engine report only; the Library UI that shows it (#124, #127) has not landed | #113 | ci: unit tests in `eak-validation/src/footprint.rs` |
| Part model, part sources and KiCad export (new crate `eak-library`) | partial: engine only; the desktop app does not use the crate yet | #109 | ci: `crates/eak-library/tests/{kicad_roundtrip,sources}.rs` |
| MPN discovery and manufacturer identification | partial: engine only (`eak-library/src/mpn.rs`); no UI | #114 | ci: unit tests in `mpn.rs` |
| App version 0.2.0 | landed | #112 | ci; `scripts/check-release-docs.sh` (versions agree) |

## Open when this page was written (not in this release unless merged and green before the freeze)

| Feature | PR |
|---|---|
| Journey bar and Problems panel | #105 |
| Agent proposal review dialog | #111 |
| Release integrity: unverified footprints block, packaging bound to the revision | #123 |
| AI tool adapters and Settings › AI tools | #125 |
| Agent activity view (WEFT) | #126 |
| Library: KiCad folder import, SQLite library database, export/import, MPN datasheets and specs, CSV/JSON part lists, pin-to-pad and verification UI | #115, #116, #118, #119, #120, #121, #122, #124, #127 |

## What is not done

- **Signing:** nothing is code-signed or notarized; the repository has no signing secrets. Windows SmartScreen and
  macOS Gatekeeper will warn, and `provenance.json` records `"signing": "unsigned (no signing secrets exist)"`.
- **Installers:** the one-line installers (`install.sh`, `install.ps1`) are code-reviewed only and have never been run
  (docs/PLATFORM_SUPPORT.md). The release workflow smoke-tests the built bundles on its runners; nobody has tested them
  on real hardware.
- **Platforms:** there are no Linux arm64 or Windows arm64 builds.
- **Product decisions:** the rail's Agents item opens Model settings, because no agent panel exists in the EDA workspace
  yet; agent sessions run in the Terminal.
- **Not shown in the app:** footprint verification, MPN discovery and the `eak-library` part model have no UI in this
  release.

## Known bugs

There are no open GitHub issues. Known limitations are listed per feature in STATUS.md and docs/FEATURE_MATRIX.md.

## Artifacts

`release.yml` publishes the following files, each listed in `SHA256SUMS`. It runs `sha256sum -c` before it publishes,
and `provenance.json` records the commit, the green ci run, the toolchains and the hashes.

| Platform | Desktop | CLI (`eak`, `eak-mcp`) |
|---|---|---|
| Linux x86_64 | `eak-linux-x86_64.deb`, `eak-linux-x86_64.AppImage` | `eak-cli-linux-x86_64.tar.gz` |
| Windows x86_64 | `eak-windows-x86_64-setup.exe` (NSIS) | `eak-cli-windows-x86_64.zip` |
| macOS arm64 | `eak-macos-aarch64.dmg` | `eak-cli-macos-aarch64.tar.gz` |
| macOS x86_64 | `eak-macos-x86_64.dmg` | `eak-cli-macos-x86_64.tar.gz` |

Release commit, ci run, release run and download checks: to be filled in after the tag.
