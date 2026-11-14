# Validation project: 12-24 V → 5 V / 3 A synchronous buck PCB, built with EAK

This folder is an independent, end-to-end use of EAK on a real power-electronics board:
a TI TPS54302 synchronous buck with reverse-polarity, fuse and TVS input protection, on a
66 × 33 mm two-layer 2 oz board.

All design data is produced by EAK. `src/design.rs` submits typed `Command`s to
`eak_runtime::Engine::execute`, the same path the UI, CLI and agent use. Electrical numbers
come from `eak-engineering`. ERC/DRC/MECH come from `eak-validation`. Gerber, Excellon, BOM
and CPL come from `eak-manufacturing`.

| Path | What |
|---|---|
| `src/parts.rs` | Component definitions (symbol, footprint, pin map, rated values with provenance), submitted via `UpsertComponentDefinition` |
| `src/design.rs` | The design: requirements, schematic, stackup, rules, placement, routing, vias, pours |
| `src/calcs.rs` | Engineering checks that call `eak_engineering::{ipc, calc, copper}` |
| `project/` | **Native EAK project**: `project.eak.json` (EAK-IR), `journal.jsonl` (every command), `evidence/` (revision-bound ERC/DRC/MECH) |
| `outputs/` | EAK manufacturing package: Gerber (copper, mask, paste, silk, outline), Excellon drill, BOM, CPL (pick-and-place), SHA-256 `manifest.json` |
| `reports/` | ERC/DRC/MECH JSON, `engineering-calcs.md`, `netlist.txt`, `connectivity.json`, `independent-inspection.txt`, `summary.txt` |
| `DESIGN.md` | Specification, assumptions, calculations, component justification, protection, insulation, layout rationale |
| `READINESS.md` | Evidence log: every test, command, result, evidence path and limitation, including what is NOT VERIFIED |
| `GAPS.md` | EAK capabilities this board needed that do not exist yet |
| `tools/` | Independent reviewer tools (POSIX sh/awk, **not EAK functionality**): `inspect.sh` checks the package files, and `render.sh` + `gerber2svg.awk` render them to SVG |

## Reproduce

```sh
cargo run --release --manifest-path validation/power-buck/Cargo.toml   # regenerates project/, outputs/, reports/
cargo test --release --manifest-path validation/power-buck/Cargo.toml
sh validation/power-buck/tools/inspect.sh validation/power-buck/outputs 66 33
sh validation/power-buck/tools/render.sh validation/power-buck/outputs /tmp/buck-svg   # any scratch directory
```

The package is a stand-alone Cargo package with its own `[workspace]` table, so the EAK
workspace manifest is unchanged. Element IDs are random UUIDs, so each regeneration produces
new IDs and new hashes, while the geometry and the netlist stay the same.

Status: EAK's internal gates (ERC, DRC and MECH, each bound to the same revision) report no
errors. That is **not** a claim that the board is electrically safe, thermally compliant or
manufacturing-ready. See READINESS.md.
