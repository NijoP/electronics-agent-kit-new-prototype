# Changelog

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in library (48 parts: generic passives, discretes, headers, crystals, switch;
  ESP32-C3-MINI-1, LMR33630 buck, SHT40, AO4406A, power inductors, SMD electrolytics, screw
  terminal with datasheet provenance), schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
