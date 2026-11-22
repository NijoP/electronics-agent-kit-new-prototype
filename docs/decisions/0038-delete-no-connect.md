# ADR-0038: `Command::DeleteNoConnect`, and at most one no-connect marker per pin
- **Date:** 2026-11-23
- **Context:** Phase 1 inventory finding F7 (slice E2, editor correctness). `SetNoConnect` added
  a new marker on every call, so clicking a pin twice with the No connect tool stored two
  markers for one pin, and there was no command to remove a marker: a pin marked by mistake kept
  its marker (and ERC treated it as intentionally unconnected) until the symbol was deleted.
  The desktop also drew every sheet's markers on every sheet. Shared-contract change (`Command`)
  per AGENTS.md, so it needs an ADR.
- **Options considered:**
  1. Make `SetNoConnect` a toggle. Rejected: one command with two opposite effects depending on
     hidden state is hard for agents and reviewers to read, and a retried command would undo
     itself.
  2. Keep `SetNoConnect` additive and only deduplicate in the UI. Rejected: agents, the CLI and
     the MCP tools call the engine directly and would still create duplicates.
  3. `SetNoConnect` refuses a pin that already carries a marker (`NothingTo`), and a new
     `DeleteNoConnect { no_connect_id }` removes one. **Chosen.**
- **Decision:**
  - `SetNoConnect { symbol_id, pin }` is refused with `NothingTo` ("already marked") when a marker
    for that symbol pin exists. Inside a `Batch` that fails the batch, like any other `NothingTo`
    step (ADR-0013).
  - `DeleteNoConnect { no_connect_id }` (JSON `{"type":"DeleteNoConnect","no_connect_id":"nc_…"}`)
    removes the marker; an unknown id is `NotFound`. Class `EngineeringImpacting` (it changes
    connectivity intent and ERC), like `SetNoConnect`. One revision and one undo step.
  - The agent tool list (eak-intelligence), the harness schematic-command list and the
    task-invalidation map (`Change::Net`) include the new command.
  - The desktop No connect tool sends `SetNoConnect` on an unmarked pin and `DeleteNoConnect` on
    a marked pin, and a sheet draws only the markers of its own symbols
    (`apps/desktop/src/tools/schematic/noConnect.ts`).
- **IR and migration:** none. `NoConnect { id, symbol_id, pin }` is unchanged, so the schema
  version stays and `migrate.rs` needs no step. A project saved before this change may still hold
  duplicate markers for one pin; they are harmless to ERC (the pin is marked either way) and can be
  removed one by one with `DeleteNoConnect`.
- **Consequences:** a caller that relied on `SetNoConnect` succeeding twice now gets a refusal it
  must treat as "already marked". No existing fixture or flow marks a pin twice.
- **Source requirement:** product spec, schematic capture (ERC no-connect remedy); Phase 1 plan
  slice E2 (`DeleteNoConnect` → ADR).
