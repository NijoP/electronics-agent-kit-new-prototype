# ADR-0037 Engine-owned project records (sourcing and assembly evidence)

Status: accepted (Phase 1 slice E3). Contract change: `eak-runtime::Engine` gains
`write_project_record`; `Command`, `CommandResponse`, report types and EAK-IR are unchanged, so
there is no schema bump and no `migrate.rs` step.

## Context
Finding F1 of the Phase 1 inventory: `parts_save_record`, `parts_import` (its evidence record),
`assembly_attest` and `assembly_import_parts_file` in `apps/desktop/src-tauri/src/parts.rs` wrote
`<project>/sourcing/*.json` and `<project>/sourcing/assembly/*.json` with direct file writes. AGENTS.md
says no code writes project files directly; these writes had no journal, no event and no actor, and
the file explorer could rename or delete them.

## Options considered
1. **Typed `Command`s that put the records into EAK-IR** (`Project.sourcing`), journalled and undoable
   by the existing diff. Rejected for now: the record types (`SourcingRecord`, `AssemblyRecord`,
   `VerificationState`, …) live in `eak-manufacturing`/`eak-validation`, which depend on
   `eak-runtime`, so `Command` cannot name them without moving roughly ten types and their inherent
   methods into `eak-core`. More importantly, supplier stock and price snapshots are volatile evidence
   about parts, not design intent: putting them in the IR would make a stock refresh a design revision,
   an undo step and a diff in every design review.
2. **Keep the direct writes and document an exception.** Rejected: it leaves the gap F1 describes.
3. **Chosen: an engine-owned writer for non-design project records.** `Engine::write_project_record`
   (actor, area, file, bytes, summary) is the only writer of the `sourcing` area:
   * refuses while recovery blocks the project, for projects without a folder, and for agent actors
     (an agent does not record supplier or assembly evidence; a person does);
   * accepts only `RECORD_AREAS` (`sourcing`) and a relative `/`-separated `.json` path whose parts
     are letters, digits, `_`, `-`, `.` with no leading dot (no `..`, no absolute path, no backslash);
   * writes atomically, appends a `RecordWrite` line (op id, actor label, file, bytes, design revision,
     summary, time in ms) to `sourcing/journal.jsonl`, and emits a `project_record_written` event;
   * does not change the design revision and adds no undo step: a record is superseded by writing a
     newer one, and the journal keeps the history of writes.

## Consequences
* `parts.rs` builds the file name and bytes with `sourcing::record_file` / `assembly::record_file`
  and hands them to the engine; `sourcing::write_record` / `assembly::write_record` remain only for
  folders that are not an open project (tests, exports).
* `sourcing` joins the explorer's engine-owned names (`workspace.rs` `ENGINE_OWNED`): readable, never
  created, renamed or deleted from the explorer.
* CI runs `scripts/check-engine-boundary.sh` (with `--self-test`): it fails when desktop or harness
  code calls `sourcing::write_record` / `assembly::write_record`, or when a desktop source file not on
  its allow list uses a filesystem write primitive. The allow list names each remaining writer and why
  it is outside a project folder (app config, the library store, caches) or is the explorer.
* Known remaining writes into a project folder that are not records: generated outputs the user asks
  for (export and the fabricator preparation bundle under `output/`, written by `eak-manufacturing`
  and the harness), the harness endpoint discovery file, and `Engine::save_evidence`. They are derived
  artefacts, not design or sourcing data, and are out of this ADR's scope.
* Record writes are not undoable from Edit › Undo. If sourcing data ever has to be undoable or diffed,
  option 1 becomes the follow-up and needs its own ADR and an IR migration.

## Source requirement
AGENTS.md "Every design mutation goes through `Engine::execute` … no code writes project files
directly"; Phase 1 inventory finding F1 (slice E3).

Date: 2026-11-23
