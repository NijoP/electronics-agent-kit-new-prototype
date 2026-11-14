# ADR-0013: `Command::Batch`, one atomic edit made of several commands
- **Date:** 2026-11-14
- **Context:** The schematic editor needs edits that are several commands to the engine but one
  action to the user: drag-moving a selection (one `MoveSymbol` per symbol plus wire-end moves),
  paste, align, delete-selection, and AI proposals. Sent one by one they take N revisions and N
  undo steps, and a failure half-way leaves a half-moved selection. Phase 2 of
  `docs/archive/eda-desktop-plan/EDA_DESKTOP_IMPLEMENTATION_PLAN.md` puts this first because the other
  editor work depends on it.
- **Options considered:**
  1. Client-side grouping: the UI sends N commands and an "undo group" id. Rejected: not
     atomic (a failure mid-way is already committed), and the engine would need group-aware
     undo.
  2. Per-feature compound commands (`MoveSelection`, `PasteSelection`, ...). Rejected: one new
     contract change per feature, and the AI cannot compose them.
  3. One generic `Batch { commands, label }` that applies its steps in order inside the
     engine's existing transaction. **Chosen.**
- **Decision:**
  - `Command::Batch { commands: Vec<Command>, label: Option<String> }` (JSON
    `{"type":"Batch","commands":[{"type":"MoveSymbol",...},...],"label":"Move 3 objects"}`).
  - `apply` runs the steps in order on the engine's working clone. A failing step fails the
    whole batch with its own error kind and the message `batch step i (Name): ...`; nothing is
    committed. The integrity check, diff, journal entry and revision bump happen once, after the
    last step, so a batch is **one revision and one undo/redo step** (undo is diff-based, so no
    per-step inverse is needed).
  - Steps are strict: a step that changes nothing (`NothingTo`) fails the batch. Callers send only
    real changes.
  - Batches do not nest; empty batches and batches over `MAX_BATCH` (1000) steps are invalid;
    the label is at most 200 characters.
  - `class()` is the riskiest step's class, so an agent approval must cover every step.
  - `created` concatenates the steps' created ids in order; each warning is prefixed with its
    step number and command name.
  - Intermediate states between steps are not integrity-checked; only the final state is.
  - AI tool: one `Batch` tool whose steps are `{"type": <tool name>, ...arguments}`, decoded by
    serde into real `Command`s like any direct tool call; domain `Mixed` asks for ERC, DRC and
    MECH afterwards. Desktop `types.ts` mirrors the variant.
- **Consequences:** A step cannot refer to an id created by an earlier step of the same batch
  (ids are generated during apply). Paste and duplicate therefore need a backend helper that
  builds the batch, as the plan already foresees. The journal records the command name `Batch`;
  the label is not yet shown in the history UI.
- **Source:** Owner master directive Part 5 and final execution directive section 3 (interactive
  editing with undo/redo); EDA desktop implementation plan, Phase 2.

## Addendum (2026-11-14): editing commands used by the select tool
- `MoveWire { wire_id, a, b }` sets both ends of a wire (whole-wire move, or one end stretched
  when an attached symbol is dragged). Zero-length is invalid; unchanged ends are `NothingTo`;
  passing over another pin warns, like `AddWire`. Class `EngineeringImpacting` (connectivity).
- `MoveNetLabel { label_id, position }`. Class `EngineeringImpacting` (a label names the net it
  touches).
- `MoveSymbol` now warns only when wire ends are actually left on its old pin positions (and no
  new pin lands there), instead of always. A drag submits
  `Batch [MoveWire..., MoveNetLabel..., MoveSymbol...]`; the wire steps come first, so the batch
  carries no stale-wire warning.
- Rubber-banding is computed by the desktop's pure reducers
  (`apps/desktop/src/tools/schematic/selection.ts`) from the snapshot's derived pin geometry; the
  plan's backend `wires_attached` helper was not needed. Wire ends move straight, so a rubber-band
  can leave a diagonal wire (no automatic corner insertion yet).
