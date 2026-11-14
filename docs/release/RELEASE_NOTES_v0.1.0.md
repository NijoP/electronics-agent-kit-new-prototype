# EAK v0.1.0 (prerelease prototype)

First packaged build of the Electronics Agent Kit desktop app. **This is an early-development
prototype.** It is not production-ready, and nothing it produces is manufacturing-ready.

## Downloads

| File | What it is |
|---|---|
| `EAK_0.1.0_amd64.deb` | Debian/Ubuntu package (x86_64). Installs `eak-desktop` and an "EAK" menu entry |
| `EAK_0.1.0_amd64.AppImage` | Portable Linux x86_64 build |
| `SHA256SUMS` | SHA-256 checksums of the two packages (`sha256sum -c SHA256SUMS --ignore-missing`) |

Install steps and system requirements: [README, "Install and run the EAK desktop app"](../../README.md#install-and-run-the-eak-desktop-app).
Windows and macOS packages are not provided in this release; build those platforms from source
with `scripts/install.ps1` / `scripts/install.sh` (both UNVERIFIED on those platforms).

## What is in this build

The desktop app (Tauri 2 shell, React/TypeScript UI) over the Rust engine, as on `main` at the
release commit. Details and the exact test record for each area are in `STATUS.md` and
`FEATURE_MATRIX.md`.

- **Workspace:** project home with recent projects, menu bar, document tabs with split view,
  resizable and persisted side/assistant/bottom panels, command palette, quick open, dark and
  light themes, status bar.
- **Schematic editor:** place library parts, wire with pin snapping, net and power labels, move,
  rotate, delete, edit reference and value.
- **PCB editor (2-16 copper layers):** board outline, place/move/rotate/flip footprints, tracks on
  any copper layer, through/blind/buried vias, plane fill, layer visibility and active layer.
- **Every edit is a typed engine command:** undo/redo, durable per-command save with a crash-safe
  journal, reopen with history.
- **Verification:** ERC (13 rules), DRC (17 rules, including multilayer DRC-016/017) and 3
  mechanical checks. Reports are revision-bound and list unimplemented checks and limitations.
- **Manufacturing export:** Gerber (one file per copper layer), Excellon drill per via span, BOM,
  CPL, `stackup.json` and a SHA-256 manifest. Marked DRAFT unless ERC, DRC and mechanical checks
  pass for that exact revision.
- **Domain intelligence:** deterministic classifier over the 100 DESK domain classes (no model
  call). Guidance is shown as unverified review prompts.
- **Model settings:** Anthropic, OpenAI-compatible and Ollama providers; API keys are kept in the
  OS keyring only. The in-app assistant is a local deterministic command interpreter; the AI agent
  loop is not wired into the desktop UI yet.

## How this release was tested

All results below are from real runs on one machine: Zorin OS 18.1 (Ubuntu 24.04 base, glibc 2.39,
x86_64, 8 GB RAM), rustc 1.97.1, Node 22.23.2, npm 12.1.0. No CI was used (GitHub Actions is disabled).

- **Build:** `bash scripts/release-build.sh` (clean `npm ci` from `package-lock.json`, `tauri build`
  with `Cargo.lock`, bundles `deb,appimage`) completed with exit code 0 and wrote both packages and
  `SHA256SUMS`. The Tauri AppImage plugin could not be downloaded during the build, so Tauri used
  its built-in AppImage tooling.
- **`.deb`:** inspected with `dpkg-deb -I/-c` (package `eak` 0.1.0 amd64; `/usr/bin/eak-desktop`,
  menu entry `EAK.desktop`, 32 and 128 px icons; depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0`).
  Extracted with `dpkg-deb -x` (not installed system-wide); `ldd` finds every library. The binary
  started on an X11/Xwayland display, mapped a 1400x900 window titled "EAK", created its recent-
  project index in a throwaway data folder, logged no errors, was still running after 12 s and
  exited on `kill`.
- **AppImage:** run directly; still running after 15 s with its bundled WebKitGTK network and web
  processes, window "EAK" mapped, window capture showed the Project Home screen (recent projects,
  open project, new project). Exited on `kill`.
- **Frontend tests:** `npx vitest run` in `apps/desktop`: 10 files, **65 passed, 0 failed**.
- **Rust tests:** not re-run for packaging. The per-feature Rust results are in `TEST_RESULTS.md`
  and `STATUS.md`, as recorded by each merged pull request.
- **Not tested:** installing the `.deb` system-wide with `apt`, any distribution other than the
  build host, interactive use of the packaged app, Windows and macOS.
- **Secrets:** the packaged binary was scanned for API-key and private-key patterns; none found.

The release page records the exact commit the published files were built from and their SHA-256
checksums.

## Known limitations

- **Prototype.** No independent engineering review and no fabricator DFM check has been done. IPC
  tables are not verified against the standard text; JLCPCB stackup limits are unverified. Exported
  Gerbers have not been checked in an external viewer or fabricated.
- **Linux x86_64 only** as packages. Windows and macOS have not been built or verified since commit
  599bf91 (CI is disabled). The packages were built and launch-checked on one Zorin OS 18.1 (Ubuntu 24.04 base) host only.
- **No real-window test suite.** Editor behaviour is covered by jsdom (vitest) and headless Rust IPC
  tests; the packaged app was only checked to start and stay running, not exercised interactively.
- **AI:** never run against a live model provider; the agent loop is not in the desktop UI.
- **Not implemented:** inner-layer autorouting, differential pairs, impedance control, per-zone
  relief settings, free-form zone drawing, text and multi-select editing, symbol/footprint
  editors, simulation, 3D viewport, Save as. DESK domain rules are listed but not evaluated.
- **Known flaky test:** `eak-runtime/tests/process.rs::captures_output_and_exit_code` is
  intermittent under load (unresolved).
- **Unsigned packages.** The AppImage has no embedded signature; verify downloads with `SHA256SUMS`.
- Saving model API keys needs a running Secret Service (GNOME Keyring or KWallet); without one,
  saving a key fails visibly.
