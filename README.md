# Electronics Agent Kit (EAK)

**Intent in. Engineered, verified electronics design out.**

EAK is a standalone, Rust-first, AI-native desktop application for electronics design, built by
Tusk Innovations Global Pvt Ltd.

> **Early development.** EAK is a prototype. It is not production-ready and nothing it produces
> should be treated as manufacturing-ready. See [Project status](#project-status).

## Install and run the EAK desktop app

There are two ways to install EAK: a prebuilt Linux package from GitHub Releases (A), or a
build from source with the installer script (B), which also covers macOS and Windows.

### A. Prebuilt Linux package (.deb or AppImage)

Starting with v0.1.0, prebuilt Linux x86_64 packages, a `SHA256SUMS` file and release notes are
attached to each release on the
[GitHub Releases page](https://github.com/NijoP/electronics-agent-kit-new-prototype/releases).
They are **prerelease prototypes**: read the release notes and known limitations first. If no
release is listed yet, use option B.

**System requirements**

- Linux x86_64 with glibc 2.39 or newer (Ubuntu 24.04 or a derivative). Built and launch-checked on
  Zorin OS 18.1 (Ubuntu 24.04 base) only; other distributions may work but are untested.
- The `.deb` depends on WebKitGTK 4.1 and GTK 3 (apt installs them). The AppImage bundles WebKitGTK.
- A Secret Service keyring (GNOME Keyring or KWallet) to store model API keys.
- No prebuilt packages for Windows or macOS yet: use option B.

**Debian / Ubuntu (.deb)**

```bash
sudo apt install ./EAK_0.1.0_amd64.deb
eak-desktop
```

EAK also appears in the applications menu. Remove it with `sudo apt remove eak`.

**AppImage**

```bash
chmod +x EAK_0.1.0_amd64.AppImage
./EAK_0.1.0_amd64.AppImage
```

If FUSE is not available, run it with `--appimage-extract-and-run`.

**Verify the download** (in the download folder): `sha256sum -c SHA256SUMS --ignore-missing`

### B. Build from source with the installer

The installer checks prerequisites,
clones (or reuses) this repository, builds the desktop app, installs a launcher and starts it. It
never runs `sudo`: if a system package is missing it stops and prints the exact command to run.
The first build takes several minutes.

**Linux and macOS** (bash):

```bash
curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.sh | bash
```

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex
```

Already have a checkout? Run `bash scripts/install.sh` (or `.\scripts\install.ps1`) from it. Use
`--no-launch` (`-NoLaunch` on Windows) to install without starting the app.

**Prerequisites** (the installer checks each one and tells you what is missing):

- Git, stable Rust 1.80 or newer ([rustup](https://rustup.rs)), Node.js 22 or newer with npm.
- **Linux:** a C compiler, `pkg-config` and the Tauri 2 libraries (webkit2gtk-4.1, GTK 3,
  libsoup 3, ayatana-appindicator, librsvg). On Debian/Ubuntu:
  `sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev`.
  The installer prints the equivalent for Fedora, Arch and openSUSE.
- **macOS:** Xcode command-line tools (`xcode-select --install`).
- **Windows:** MSVC build tools (C++ workload) and the WebView2 runtime; the installer prints the
  `winget` commands.

**Where it installs and how to launch it**

| OS | Binary | Launcher |
|---|---|---|
| Linux | `~/.local/bin/eak-desktop` | "EAK" in the applications menu (`~/.local/share/applications/eak.desktop`), or run `eak-desktop` |
| macOS | `~/Applications/EAK.app` (unsigned: right-click, Open the first time) | Finder / Launchpad |
| Windows | `%LOCALAPPDATA%\EAK\eak-desktop.exe` | "EAK" in the Start Menu |

**Add a model API key.** Open EAK, click **Model settings**, choose a provider (Anthropic,
OpenAI-compatible or Ollama), enter the key and click **Save**, then **Test connection**. The key is
stored in the OS keyring, never in a project or profile file. The AI agent is not yet wired into
the desktop UI (see [Features](#features)).

**Platform status (option B):** Linux verified (2026-11-14), Windows UNVERIFIED, macOS UNVERIFIED.
"Verified" means `scripts/install.sh` was run from a fresh clone on one Linux machine and the app
process stayed up; it is not a claim about every distribution.

## What is EAK

EAK is an electronics design automation (EDA) platform that treats AI as a first-class user of
the same tools a human engineer uses, without letting it grade its own work.

- **One canonical design model.** Every schematic, PCB, library and manufacturing setting lives in
  a single representation, **EAK-IR** (`crates/eak-core`). There is no second copy to drift out
  of sync.
- **One typed command engine.** Every design change is a typed `Command` executed through
  `eak_runtime::Engine::execute`. The desktop UI and the AI agent submit the same commands
  (a CLI is planned). Changes are revision-checked, transactional, undoable and journalled.
- **Independent verification.** Engineering numbers are recomputed by deterministic Rust engines
  (`eak-engineering`), and checks run in a separate engine (`eak-validation`). A check that is
  not implemented is reported as not implemented, never as passing.
- **Standalone.** EAK owns its engineering model. It does not wrap EasyEDA or KiCad and has no
  mandatory Freerouting dependency.
- **Cross-platform.** Windows, Linux and macOS are equal targets, from a Rust core with a
  Tauri 2 + React/TypeScript desktop shell.

## Features

What exists on `main` today. [STATUS.md](STATUS.md) records exactly what has been tested and what
has not.

| Area | What it does | Maturity |
|---|---|---|
| **Schematic editor** | Place library parts, wire with pin snapping, connect pins, net and power labels, move, rotate, delete, edit reference and value | Tested headless (vitest + Rust IPC). Not yet exercised in a real window |
| **PCB editor (2-16 layers)** | Board outline (rectangle or polygon), place, move, rotate and flip footprints, route tracks on any copper layer, through/blind/buried vias, one-click plane fill, layer visibility and active layer | Same as above. No free-form zone drawing, text or multi-select editing yet |
| **Undo, save, reopen** | Undo/redo for every command; each command is durably saved with a crash-safe journal; projects reopen with their history | Covered by engine contract tests |
| **ERC and DRC** | Electrical rules (13) and design rules (13), plus 3 mechanical checks. Findings carry a stable id, affected objects, location, suggested fix and measured vs required values | Reports are bound to a revision and an input hash and list unimplemented checks and limitations |
| **Manufacturing export** | Gerber (RS-274X), Excellon drill, BOM and CPL with a SHA-256 manifest and package re-verification | Output is marked DRAFT unless ERC, DRC and mechanical checks pass for that exact revision. Never called manufacturing-ready. Not yet viewed in an external viewer or sent to a fabricator |
| **Engineering engines** | IPC-2221 trace width, clearance, creepage and via current; circuit calculators; netlist resolution; a native two-layer router; a constructive placer | IPC tables are **not** verified against the standard text |
| **Universal model adaptor** | Configure Anthropic, OpenAI-compatible (OpenAI, OpenRouter, Groq, Together, local servers) or Ollama providers. API keys are stored in the OS keyring, never in project or profile files. List models and test the connection | Never run against a live provider. Windows and macOS keyrings not exercised |
| **AI agent core** | Typed tool per command, read-only design and calculator tools, bounded agent loop with Manual, Assisted and Autonomous modes, sandbox preview and approval policy | Tested with a scripted provider. Not yet wired into the desktop UI |

## Architecture

```
                 ┌────────────┐   ┌──────────────┐
                 │ Desktop UI │   │  AI agent    │      same typed Command values
                 │ Tauri 2 +  │   │ (eak-        │
                 │ React / TS │   │ intelligence)│
                 └─────┬──────┘   └──────┬───────┘
                       └────────┬────────┘
                         ┌──────▼───────┐
                         │ eak-runtime  │  Engine::execute: revisions, approval policy,
                         │              │  transactions, undo/redo, journal, recovery
                         └──────┬───────┘
                         ┌──────▼───────┐
                         │   eak-core   │  EAK-IR: the one canonical design model
                         └──────────────┘
      eak-engineering · eak-validation · eak-manufacturing operate on that one model
```

| Path | Purpose |
|---|---|
| `crates/eak-core` | EAK-IR, geometry, library model, schema and migration, integrity, built-in parts |
| `crates/eak-engineering` | IPC calculations, circuit calcs, netlist, copper model, router, placer, 3D scene data |
| `crates/eak-validation` | ERC, DRC, mechanical checks, revision-bound evidence |
| `crates/eak-runtime` | Command engine, undo, journal and recovery, process supervisor, tasks, SQLite index |
| `crates/eak-manufacturing` | Gerber, Excellon, BOM, CPL compiler, manifest, package re-verification |
| `crates/eak-intelligence` | Provider adaptors, typed tools, agent loop |
| `apps/desktop` | Tauri 2 shell and React/TypeScript UI |
| `docs/` | Architecture, requirements traceability, ADRs, engineering rules |

More detail: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/PLATFORMS.md](docs/PLATFORMS.md).

## Getting started

### Prerequisites

- Stable Rust (1.80 or newer) and Node.js 22 or newer with npm.
- **Linux:** `sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
- **macOS:** Xcode command-line tools (`xcode-select --install`).
- **Windows:** the WebView2 runtime, plus either the MSVC build tools or a MinGW-w64 GCC
  (`source scripts/env.sh` in Git Bash).

No Python is required or permitted anywhere in this repository.

### Build and run the desktop app

```bash
cd apps/desktop
npm ci
npm run tauri dev
```

A release binary with the embedded frontend: `npm run tauri -- build --no-bundle`. To build the
Linux installers (`.deb` and AppImage) with a `SHA256SUMS` file, run `bash scripts/release-build.sh`
from the repository root; the output lands in `target/release-artifacts/`.

### Test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
bash scripts/check-no-python.sh
(cd apps/desktop && npm test)
```

### Command line

A standalone `eak` CLI is not on `main` yet. The engine is also usable as a Rust library
(`eak-runtime`), as the integration tests demonstrate.

## Project status

EAK is in early development. "Implemented" means code exists and its listed tests pass, not that
it has been professionally validated. In particular:

- No independent engineering review or fabricator DFM check has been performed.
- Output is **not production-ready and not manufacturing-ready**.
- Differential pairs, impedance control and inner-layer autorouting are not implemented.

See [STATUS.md](STATUS.md) for the verified record and [ROADMAP.md](ROADMAP.md) for what is next.

## Contributing

Read [AGENTS.md](AGENTS.md) for the working rules (one canonical IR, every mutation through the
command engine, no Python, no unverified claims) and [TESTING.md](TESTING.md) for the test
levels. Every bug fix needs a regression test, and changes to shared contracts need an ADR in
[docs/decisions](docs/decisions).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.

Copyright 2026 Tusk Innovations Global Pvt Ltd. Third-party dependency licences are listed in
[LICENSING.md](LICENSING.md).
