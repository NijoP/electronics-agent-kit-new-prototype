# Changelog

## 0.1.0 (unreleased) - M1.1 CLI and CI matrix
- New `eak` command-line interface (`apps/cli`), a thin client of the shared command engine.
  - Subcommands: new, demo, open, show, apply (JSON command files or stdin, `@REFDES` references, `--expect-rev`), undo, redo, check erc|drc|mech|all, compile, checkpoint, checkpoints, restore, acknowledge-recovery.
  - `--json` mode and stable exit codes (0 ok, 1 failure, 2 usage, 3 verification errors / draft package).
- Fix: the UNKNOWN_OUTCOME recovery write block now persists across sessions (`recovery.blocked`) until acknowledged. Previously, reopening the project lifted it silently.
- Fix: the Unix process-tree kill now uses `killpg(SIGKILL)` on the supervised process group.
- Fix: `Supervisor::wait` drains captured output, so fast-exiting processes no longer lose it.
- Fix: lock-file I/O errors are no longer misreported as "project is locked".
- CI: shell scripts committed executable; the retired `macos-13` runner was replaced by `macos-15-intel`.

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
