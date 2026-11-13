# Changelog

## Unreleased - thermal reliefs, silkscreen text, IPC value register
- Zone fill: thermal-relief gap and four axis spokes for every same-net pad. Spokes that cannot
  land are removed. The fill, the connectivity analysis and the copper Gerber share the same
  relief geometry.
- Silkscreen: reference designators in an original built-in stroke font (mirrored on the bottom
  side). The whole legend is clipped 0.1 mm clear of soldermask openings and written to the
  legend Gerbers.
- DRC-014, thermal-relief spokes: fewer than 2 landed spokes is a warning; a spoke narrower than
  the minimum track width is an error; spoke width below the IPC-2221 width for the declared net
  current is a warning.
- DRC-015, silkscreen over soldermask openings: overlap is an error, closer than 0.1 mm is a
  warning.
- `docs/engineering/IPC_RULE_VALUES.md` lists every IPC/IEC value with its verification status.
  All of them are unverified.
- The end-to-end reference board covers reliefs and legend text, and still reaches
  InternallyVerifiedReleaseCandidate.

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
