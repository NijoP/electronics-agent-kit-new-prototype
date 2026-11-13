# EAK architecture

Governing sources: Spec §2, §7, §8.1; Execution Instructions §5–7. A modular monolith: one
desktop process, one Rust runtime, a small set of crates with one-directional dependencies.

```
             +-------------------------+     +--------------------------+
 Desktop UI  | apps/desktop (Tauri 2 + |     | eak-intelligence (lead   |
             | React/TS) — presentation|     | agent, typed tools)      |
             +-----------+-------------+     +------------+-------------+
                         | CommandRequest (typed)         | CommandRequest (Agent actor + approval)
                         v                                v
             +---------------------------------------------------------+
             | eak-runtime  Engine::execute                            |
             |  revision check → approval policy → apply on clone →    |
             |  integrity → diff (delta) → journal pending → atomic    |
             |  write → journal committed → event                      |
             |  undo/redo · checkpoints · recovery · processes · tasks |
             +---------------------------+-----------------------------+
                                         | &mut Project (only here)
             +---------------------------v-----------------------------+
             | eak-core  EAK-IR (single source of truth)               |
             +---------------------------+-----------------------------+
                  read-only              |               read-only
        +----------------------+   +-----v--------------+   +---------------------+
        | eak-engineering      |   | eak-validation     |   | eak-manufacturing   |
        | calcs, netlist,      |-->| ERC / DRC / MECH   |-->| Gerber, drill, BOM, |
        | copper, router,      |   | revision-bound     |   | CPL, manifest,      |
        | placer, 3D scene     |   | evidence           |   | release gate        |
        +----------------------+   +--------------------+   +---------------------+
```

`apps/desktop` exists (thin Tauri shell over `Engine`; see `apps/desktop/README.md`). `eak-intelligence` is planned.

## Key decisions (see `docs/decisions/`)
- **ADR-0001 Project format:** a folder holding `project.eak.json` (canonical, pretty and
  byte-stable JSON) and `journal.jsonl`, plus the `checkpoints/`, `evidence/` and `outputs/` folders.
  SQLite holds only rebuildable metadata (recent projects, event log).
- **ADR-0002 Identity:** stable, prefixed random ids (`sym_`, `fp_`, `trk_`...). A footprint carries
  `symbol_id`, which is the cross-probe key across schematic, PCB and 3D. Nets are derived from
  connectivity and never stored as a second truth; PCB pad nets are synchronised from the schematic,
  and DRC-010 flags any drift.
- **ADR-0003 Transactions and undo:** a command is applied to a clone and must pass the integrity
  check. A section-level JSON diff is then stored as the reversible delta. Undo and redo apply the
  delta's before/after values and are new revisions (history is never rewritten). A property test
  checks that undoing every command kind restores the exact prior state.
- **ADR-0004 Approval policy:** risk classes are ReadOnly, ReversibleLocal, EngineeringImpacting and
  Destructive. Human edits authorise themselves (the UI confirms destructive actions). Agent
  requests need an `Approval` covering the class.
- **ADR-0005 Geometry:** integer nanometres and quarter-turn rotations, for exact, cross-platform
  deterministic results.
- **ADR-0006 Python-free, cross-platform toolchain:** stable Rust and npm only. CI covers Windows,
  Ubuntu, macOS arm64 and macOS x64.

## Crash safety
Write order per commit:
1. Append a `pending` journal record containing the delta (fsync).
2. Atomically replace `project.eak.json` (temp file, fsync, rename).
3. Append a `committed` record.

On open, each pending operation is reconciled against the on-disk revision:
- Revision equals `revision_after`: the operation is marked committed.
- Revision equals `revision_before`: the operation is marked aborted.
- Anything else: UNKNOWN_OUTCOME, which blocks writes until the user acknowledges the state.
