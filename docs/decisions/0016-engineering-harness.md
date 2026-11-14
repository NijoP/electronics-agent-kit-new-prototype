# ADR-0016: Engineering harness for external coding agents

Status: accepted (2026-11-15)

## Context
External CLI agents (Pi, OpenCode, Claude Code, ...) running in EAK's terminal must read and
change real EAK projects and get deterministic validation back, without a second design model,
direct file writes, or a global "active project" (product vision §5, Phase 1 directive §7).

## Decision
* New crate `crates/eak-harness` with binaries `eak` (CLI, `--json`) and `eak-mcp` (= `eak mcp`,
  MCP over stdio). Both call one dispatcher, `eak_harness::handle(&mut Engine, ...)`.
* Tools are categorised `read`, `design`, `simulation`, `validation`, `manufacturing` (plus
  `project`, `discovery`). Design tools are exactly the `eak_runtime::Command` variants (schemas
  reused from `eak-intelligence`) plus Undo/Redo; each needs `expected_revision`, and a mismatch
  is the engine's `StaleRevision`, returned as `stale_revision`.
* Every call names its project explicitly (`--project`/`project_dir`, default `EAK_PROJECT_DIR`),
  may pin the project id (`EAK_PROJECT_ID` → `project_mismatch`), and every response echoes the
  project id and revision.
* **Single writer.** While the desktop app has a project open it serves the same tools on a
  token-authenticated local endpoint (Unix socket in a 0700 dir; loopback TCP on Windows) and
  advertises it in `<project>/.harness.json` (no secret). The CLI/MCP routes through it, so the
  app's engine applies the change and the editor re-renders from its events. Otherwise the CLI
  opens the project directly under the existing exclusive `.lock` for one call.
* **Shared-contract change:** `eak_runtime::Actor` gains `session: Option<String>` (serde default,
  omitted when `None`); `Actor::label()` becomes `agent:<name>@<session>` when set. Existing
  journals and actors without a session are unchanged. Harness edits are journaled with
  `Actor::agent_in_session(agent, session)` and a standing `Approval` bounded by the session
  policy: up to `engineering_impacting` by default, `destructive` only when the user starts the
  agent with `EAK_HARNESS_ALLOW_DESTRUCTIVE=1` / `--allow-destructive`.
* Each call is appended to `<project>/harness-calls.jsonl`; validation reports are stored as
  evidence. `get_history` exposes journal, calls and evidence.

## Consequences
* No new mutation path: the harness only calls `Engine::execute/undo/redo` and the existing
  validators and manufacturing compiler.
* The approval of a harness session is a standing, user-granted policy, not a per-command human
  confirmation; destructive commands remain opt-in.
* Not yet: per-call human approval UI for agent edits, Windows named pipes, the autonomous design
  loop (requirement → spec → schematic → PCB iterations).
