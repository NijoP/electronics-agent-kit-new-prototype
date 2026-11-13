# Changelog

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (16 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
- Synchronous-buck power-stage calculations (duty, inductor, ripple, peak/RMS current, capacitors,
  MOSFET/inductor losses and efficiency, E24/E96 feedback divider) with sourced formulas
  (`docs/engineering/POWER_CALCULATIONS.md`).
- ERC: shared component rating check. ERC-011 (capacitor voltage) now reports a missing rating or
  rail as not checked. New rules: ERC-014 regulator/IC supply voltage, ERC-015 inductor current,
  ERC-016 MOSFET Vds.
