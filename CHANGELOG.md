# Changelog

## Unreleased
- EAK-IR schema v2: per-layer board stackup (`StackupLayer`: layer, copper weight, dielectric
  thickness, er) with a v1 → v2 migration; v1 files load with identical meaning (ADR-0008).
- DRC-008 track current capacity uses the copper weight of the track's own layer.
- `SetStackup` and the load integrity gate validate the stackup (layer order, positive values,
  build-up within ±10 % of the declared board thickness).

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
