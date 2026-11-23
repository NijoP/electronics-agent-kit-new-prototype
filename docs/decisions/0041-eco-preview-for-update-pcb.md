# ADR-0041: ECO preview for Update PCB (read-only sync preview, apply as a Batch)
- **Date:** 2026-11-23
- **Context:** Phase 1 slice C2 (plan gaps #25, #26; AD-9). "Update PCB from schematic" ran
  `SyncSchematicToPcb` directly and only reported afterwards what it had changed. Spec AG-02 asks
  that consequential operations show their scope before they run, and the EasyEDA behaviour study
  shows the reviewed change list ("ECO") users expect: changes grouped by action or object, a
  checkbox per change, an exportable report, then apply. This adds a desktop IPC and a report
  file; it does not change EAK-IR, `Command`, `CommandResponse` or a verification report type.
- **Options considered:**
  1. A second, hand-written diff that predicts what the sync would do. Rejected: two
     implementations of one rule drift apart, and the preview could then show something other
     than what is applied.
  2. A new `Command` for selective sync (e.g. `SyncSchematicToPcb { only }` or per-footprint
     place / delete / set-pad-net commands) so unchecked changes can be left out. Needed for
     partial apply, but `Command` belongs to slice C4 in Phase 1; not added here.
  3. Run the engine's own command code on a copy of the project and return the delta. **Chosen**
     for the preview; apply stays the existing command.
- **Decision:**
  - `eak_desktop::eco::preview_project(&Project)` clones the project, runs
    `command::apply(SyncSchematicToPcb)`, the integrity check and `delta::diff`, exactly as
    `Engine::execute` does, and returns `SyncPreview { revision, changes, warnings }`. `changes`
    has the same `{path, key, before, after}` shape as `operation_changes`. The only value that can
    differ from the applied delta is the random id of a footprint the sync creates.
  - IPC `sync_preview(expected_revision)` refuses a stale revision and a project blocked by
    recovery, like `Engine::execute`. It changes nothing.
  - IPC `sync_preview_export(expected_revision, excluded)` recomputes the preview and saves it as
    `evidence/eco-rNNNNNN.json` (`kind: "eco_preview"`, the change list, the sync's warnings and
    the ids of the rows the user left unchecked). The report is the engine's preview, not the UI's
    rows. It needs a project folder; the `eco-` prefix is not read by any check or gate.
  - The toolbar's Update PCB runs the ERC gate (ADR-0036), then opens the review dialog
    (`shell/EcoPreviewDialog.tsx`, rows from `shell/ecoPreview.ts`). Rows: Add / Delete / Update
    of Footprint, Reference, Part and Pad net. A pad net of a footprint being added or deleted
    depends on it: unchecking the footprint unchecks its pad nets, checking such a pad net checks
    its footprint.
  - Apply sends `Batch { commands: [SyncSchematicToPcb] }` at the previewed revision, so a project
    that moved on since the preview is rejected with `StaleRevision` (AG-03) and nothing changes.
    The Update PCB report is then built from the operation's journalled changes, as before.
  - **Partial apply is not available yet.** While any change is unchecked, Apply is disabled and
    the dialog says why. Unchecked changes are therefore never applied, but the user cannot apply
    only some of them. Selective apply needs a new command (option 2), requested from slice C4.
  - Back-annotation (PCB → schematic reference and value changes) is a later slice.
- **Consequences:** the preview equals the applied delta by construction; the property test
  `apps/desktop/src-tauri/tests/eco.rs` checks it on random edit sequences (and stale rejection,
  read-only preview, export). Agents keep using `SyncSchematicToPcb` through the harness unchanged.
