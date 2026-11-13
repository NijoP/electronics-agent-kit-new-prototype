# EAK development status

Truthful record of what exists and what has been verified. "Implemented" means code exists and
the listed tests pass; it does not mean professionally validated.

## M1.1: `eak` CLI and three-OS CI (2026-11-13)

**Platform tested locally:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`.
**Linux and macOS (CI, executed):** the matrix is windows-latest, ubuntu-latest,
macos-latest (Apple Silicon) and macos-15-intel. GitHub Actions run
[43431745631](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/43431745631)
on commit `5cf01d4` (the code of this change; the commit that records this result
changes documentation only) passed on all four jobs: Python-free check, `cargo fmt`,
`cargo clippy -D warnings` and `cargo test --workspace --release` with
**43 passed, 0 failed** on each. It is the first run to reach the build and test steps
on Linux and macOS (earlier runs stopped at the Python-free step, see below). Nothing
has been run on a Linux or macOS machine outside CI.

### Implemented and tested (Windows locally; Windows, Linux and macOS in CI)
- **`eak` CLI** (`apps/cli`, binary `eak`). It is a thin client of the shared engine:
  mutations go only through `Engine::execute` (or the engine's undo/redo/restore), and
  each invocation takes the project lock and releases it on exit. Subcommands:
  - `new`, `demo` (reference design built through commands), `open`, `show` (canonical EAK-IR).
  - `apply` (a JSON command or array, from a file or stdin):
    - Every command is schema-checked before any runs.
    - Each command is one transaction; the batch stops at the first rejection.
    - `--expect-rev` gives optimistic concurrency.
    - `"@R1"` in `symbol_id`/`a_symbol`/`b_symbol`/`footprint_id` resolves to the object with that refdes.
  - `undo`, `redo`.
  - `check erc|drc|mech|all`: revision-bound evidence is written to `evidence/`.
  - `compile`:
    - Fresh ERC/DRC/MECH on the current revision, then the package, then on-disk re-verification.
    - Refuses to overwrite an existing package without `--overwrite`.
  - `checkpoint`, `checkpoints`, `restore`, `acknowledge-recovery`.

  Output modes and exit codes:
  - `--json` prints exactly one JSON object with `ok` and `exit_code`.
  - Exit codes: 0 ok, 1 failure/rejected, 2 usage, 3 verification errors or DRAFT package.
  - There is no save step: every accepted command is persisted atomically.
- **Durable UNKNOWN_OUTCOME write block** (bug fix). The block used to live only in
  memory, so the next session (for example the next CLI call) could edit without an
  acknowledgement. It now persists in `recovery.blocked` until
  `Engine::acknowledge_recovery` (`eak acknowledge-recovery`) is called.
- **Process supervisor** (`process.rs`):
  - Unix tree kill now uses `killpg(SIGKILL)` on the child's own process group, the
    equivalent of Windows `taskkill /T /F`. It no longer depends on a `kill` binary.
  - `wait()` now drains captured output (bounded to 2 s), so fast-exiting processes no
    longer lose stdout.
- **Lock errors**: a missing folder or permission error is reported as itself, not as
  "locked by process".
- **CI**:
  - `scripts/*.sh` are committed executable (they were 100644, which failed Linux and macOS).
  - The retired `macos-13` label was replaced by `macos-15-intel`.

### Tests executed (exact results)
`cargo test --workspace --release`: **43 passed, 0 failed** on Windows locally and on
each of the four CI jobs of run 43431745631 (windows-latest, ubuntu-latest, macos-latest,
macos-15-intel). By crate:
- eak-cli: 5 end-to-end tests through the real binary.
- eak-core: 7.
- eak-engineering: 11.
- eak-manufacturing: 3.
- eak-runtime: 11 engine + 6 process.

`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
`scripts/check-no-python.sh`: OK.

The CLI end-to-end tests run each step as a new process. On the reference design they cover:
- demo build, ERC/DRC/MECH with 0 errors, checkpoint creation and listing;
- an injected short detected by DRC (exit 3) and a DRAFT package (exit 3);
- undo, then a clean check and an InternallyVerifiedReleaseCandidate package with 12 files that re-verify;
- refusing to overwrite a package, redo, checkpoint restore, and reopening.

Other CLI tests cover:
- a scripted 3-part LED board in one `@REFDES` batch (3/3 connections routed);
- batch stop-on-reject and stale `--expect-rev`;
- malformed input that leaves the project untouched;
- the lock and `--take-over`;
- usage errors (exit 2);
- the UNKNOWN_OUTCOME block persisting across invocations until acknowledged.

New process tests:
- `cancel_terminates_the_whole_process_tree`: the grandchild must die. Unix uses
  `sh` + `sleep`; Windows uses PowerShell `Start-Process`. The Unix `killpg` path
  passed on ubuntu-latest, macos-latest and macos-15-intel in run 43431745631.
- `fast_exit_output_is_not_lost`.

### Known limitations
- A multi-command `apply` batch is not atomic: commands accepted before a rejection stay
  applied. Each one is individually undoable, and the JSON reports `applied`/`total`.
- `@REFDES` resolution covers only the four id fields above. Wires, labels, tracks,
  vias and zones are still addressed by id (read them via `eak show`).
- Unix tree kill reaches the child's process group only. A descendant that calls `setsid`
  or `setpgid` to leave the group is not killed (same class of limit as `taskkill /T`
  for re-parented processes).

## Milestone M1: engineering foundation and headless vertical slice (2026-11-13)

**Platform tested:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`, WinLibs GCC 16.2,
AMD Ryzen 7 7840HS, 15.3 GB RAM. Linux and macOS: CI workflow added, **not yet run**.

### Implemented and tested
- EAK-IR v1 (`eak-core`): project, library, schematic, PCB, constraints, manufacturing config;
  integer-nanometre geometry; byte-stable JSON; schema version gate (newer/corrupt files are
  rejected safely); integrity checker; 7 built-in generic components.
- Engineering core (`eak-engineering`):
  - IPC-2221 trace width, clearance, creepage and via current, re-implemented from the DESK
    audit, with DESK reference vectors as tests (tables **not verified against the standard text**).
  - Circuit calculations.
  - Schematic netlist resolution.
  - Exact copper geometry and connectivity, with zone fill on a 0.1 mm raster.
  - A native two-layer A* router.
  - A constructive placer.
  - 3D scene data.
- Verification (`eak-validation`):
  - ERC: 13 rules.
  - DRC: 13 rules, including current capacity and voltage clearance.
  - Mechanical: 3 rules.
  - Every report is bound to a revision and a SHA-256 input hash, and lists the checks that are not implemented.
- Runtime (`eak-runtime`):
  - Typed command set (38 commands).
  - Revision checks and the agent approval policy.
  - Transactional apply with integrity gate, diff-based undo/redo and an fsync'd journal with crash reconciliation (UNKNOWN_OUTCOME blocks writes).
  - Checkpoints, project lock, evidence store.
  - Project-scoped process supervisor (timeout, cancel, bounded output).
  - Task model and SQLite index.
- Manufacturing (`eak-manufacturing`):
  - Outputs: RS-274X Gerber (9 layers), Excellon, BOM, CPL.
  - SHA-256 manifest.
  - Release gate that requires revision-bound ERC/DRC/MECH with zero errors, otherwise the package is marked DRAFT.
  - Package re-verification that detects tampering.

### Tests executed (exact results)
`cargo test --workspace --release`: **35 passed, 0 failed** (eak-core 7, eak-engineering 11,
eak-runtime 10 engine + 4 process, eak-manufacturing 3 end-to-end).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.

The key end-to-end test (`reference_design_end_to_end`) runs a 10-component ATtiny85 + AMS1117 board through
the command API:
1. Schematic, then ERC: 0 errors.
2. Sync, place, auto-route (**20/20 connections**) and a GND pour.
3. DRC: 0 errors; MECH: 0 errors.
4. Package status: InternallyVerifiedReleaseCandidate; the package re-verifies and tampering is detected.
Injected short and clearance violations are detected (`drc_detects_injected_short_and_clearance_violation`).

### Not yet implemented (acceptance gaps)
- **Desktop application** (Tauri 2 + React): not started. There is no GUI yet.
- AI Intelligence Engine (agent loop, model providers): not started.
- Symbol/footprint editors, 3D viewport UI, Terminal Center UI, research browser, LCSC/JLCPCB integrations.
- Multi-layer (>2) boards, differential pairs, impedance, thermal reliefs, silkscreen text.
- Linux/macOS: built and tested in CI only, starting with the M1.1 change (see above); packaging and installers do not exist yet.
- Independent engineering review and fabricator DFM: not performed (required before any manufacturing claim).

## Coordination note
The Firstmate crew could not be started on this host: the Firstmate session lock does not
acquire under native Windows Git Bash (upstream fix pending), and no WSL distribution is
installed. Development is proceeding directly in this repository.

## Next dependency-ready work
1. Tauri 2 desktop shell + React workspace bound to the command engine (Phase 1/4).
2. Schematic and PCB editor canvases driving the same commands.
3. Intelligence Engine: provider abstraction, typed tools over commands, approval flow.
