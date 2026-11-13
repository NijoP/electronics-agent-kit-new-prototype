# Changelog

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
- LCSC sourcing, offline only: provenance-carrying sourcing records with search-hit, datasheet-matched
  and verified states; `SOURCING` report (12 rules) for missing data, BOM-to-LCSC mapping and
  symbol, footprint and package consistency; synthetic offline fixtures. No network client.
- Access and terms research for LCSC and JLCPCB (`docs/engineering/sourcing.md`) and a separate
  integration test matrix (`docs/engineering/INTEGRATION_TEST_MATRIX.md`).
