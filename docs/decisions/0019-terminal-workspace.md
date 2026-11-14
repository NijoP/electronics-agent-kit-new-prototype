# ADR-0019: Terminal workspace on EAK's own PTY engine

Status: accepted (Phase 1)

## Context
EAK needs a first-class Terminal workspace with real PTYs, spaces bound to project roots,
tabs, splits, process status and a control API, on Windows, macOS and Linux, without copying
or bundling another terminal product.

## Decision
- A new crate `crates/eak-terminal` (portable-pty, MIT) owns all sessions; the Tauri backend
  holds one `Manager` for the app's lifetime and streams output as events; the UI uses xterm.js.
- Status comes from process facts first, agent self-reports second, labelled heuristics last.
- A token-authenticated loopback API (and `eak-term` CLI) exposes the same operations to
  agents; it is separate from the engineering harness CLI.
- No `Command`, IR or `CommandResponse` change: terminal processes are user programs outside
  the design engine.

## Consequences
Processes end with the app; the saved layout lists them as ended so they can be restarted.
See `docs/TERMINAL_ARCHITECTURE.md` and `docs/terminal-api.md`.
