# Electronics Agent Kit (EAK)

**Intent in. Professionally engineered, verified electronics design out.**

EAK is a standalone, Rust-first, AI-native electronics design automation (EDA) desktop
application by Tusk Innovations Global Pvt Ltd. It owns its own engineering model and does not
wrap EasyEDA, KiCad or any other ECAD tool, and it has no mandatory Freerouting dependency.

> **Status: early development.** See [STATUS.md](STATUS.md) for what is implemented and what has
> actually been verified. Nothing in this repository is a manufacturing-ready claim.

## Development handoff (continue on Linux)

Development moved from a Windows laptop to a Linux laptop on 2026-11-13. **Start here:**
[docs/handoff/LINUX_HANDOFF.md](docs/handoff/LINUX_HANDOFF.md). It contains the state at handoff, Linux
setup, and the **master prompt** for the Linux Firstmate that recreates the secondmate crew.
The governing delivery directive is [docs/handoff/FINAL_DIRECTIVE_2026-11-14.md](docs/handoff/FINAL_DIRECTIVE_2026-11-14.md).

**Unfinished work at handoff** (details, owners and next actions in the handoff file):

- Open, unmerged pull requests:
  - green: #12 desktop shell (awaits desktop dependency approval), #11 CLI + CI, #9 ESP32 acceptance
    project, #3 intelligence core;
  - red, needs rebase or re-run: #10 power acceptance project, #1, #4, #5, #6, #7, #8.
- GitHub Actions currently refuses to start jobs: the account's billing or spending limit must be fixed
  before any check can pass.
- Not started yet:
  - design system and interaction architecture;
  - editor-enabling typed commands;
  - interactive schematic editor and 2-layer PCB editor;
  - actionable ERC/DRC findings;
  - full LCSC integration and JLCPCB preparation;
  - universal AI providers and the assistant dock;
  - Windows/Linux/macOS installer packaging;
  - both acceptance projects built through the app.
- Deferred by the directive: 3D workspace, Terminal Center, Model Provider Center UI.

## Source of truth

The product is governed by the three documents in [`docs/source-of-truth/`](docs/source-of-truth/):

1. `EAK_Development_Intelligence_and_Interface_Research_Specification.pdf`: what EAK must be.
2. `EAK_Claude_Code_Execution_Instructions.pdf`: how development is conducted.
3. `EAK_GitHub_Reference_Repository_Development_Guide.pdf`: how reference projects may be used.

Every other document in this repository is subordinate to them.

## Architecture in one paragraph

There is one canonical engineering representation (**EAK-IR**, `crates/eak-core`). It is
mutated only by one typed **command engine** (`crates/eak-runtime`). That engine checks
revisions, enforces the approval policy, applies transactions, keeps a crash-safe journal and
supports undo/redo. Humans (desktop UI, CLI) and the AI agent submit the same command values.
Deterministic engineering engines (`crates/eak-engineering`) and an independent verification
engine (`crates/eak-validation`) operate on that one model. The manufacturing compiler
(`crates/eak-manufacturing`) emits revision-bound Gerber, drill, BOM and CPL packages.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Platforms

Windows 10/11, Linux (Ubuntu and other major distributions) and macOS (Apple Silicon and
Intel) are first-class targets from one shared codebase. CI builds and tests all three.
Platform notes: [docs/PLATFORMS.md](docs/PLATFORMS.md).

## Build and test

Prerequisites: stable Rust (1.80+). On Windows with the GNU Rust toolchain, a MinGW-w64 GCC is
also needed (the bundled SQLite and, later, Tauri resources are compiled with it). No Python is
required or permitted.

```bash
# Windows Git Bash only: put cargo + MinGW GCC on PATH
source scripts/env.sh

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
scripts/check-no-python.sh
```

## Repository map

| Path | Purpose |
|---|---|
| `crates/eak-core` | EAK-IR, geometry, library model, schema/migration, integrity, built-in parts |
| `crates/eak-engineering` | IPC calculations, circuit calcs, netlist, copper/zone model, router, placer, 3D scene |
| `crates/eak-validation` | ERC, DRC, mechanical checks, revision-bound evidence |
| `crates/eak-runtime` | Command engine, deltas/undo, journal & recovery, process supervisor, tasks, SQLite index |
| `crates/eak-manufacturing` | Gerber/Excellon/BOM/CPL compiler, manifest, package re-verification |
| `docs/` | Architecture, requirements traceability, research register, ADRs, engineering rules |
