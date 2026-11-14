# ADR-0016: Terminal workspace = supervised Herdr sidecar + EAK PTY bridge

Status: accepted (Phase 1)

## Context
The Terminal workspace must be built on the actual Herdr, usable inside EAK, with real PTYs,
spaces, splits, agent status and persistence on Windows, macOS and Linux.

## Decision
- Herdr runs as a separate, unmodified program on EAK's own session (`eak-desktop`, never
  `default`), supervised by the Tauri backend (`eak_terminal::herdr::Supervisor`). Herdr's
  interactive client runs in a PTY owned by EAK (`crates/eak-terminal`) and is drawn with
  xterm.js in the main area. Evidence and limits: `docs/HERDR_INTEGRATION.md`.
- `crates/eak-terminal` keeps a small PTY engine (sessions, status, saved layout, token API)
  because a webview has no terminal for Herdr's client, and as the fallback without Herdr.
- No `Command`, IR or `CommandResponse` change: terminal processes are user programs outside
  the design engine.

## Consequences
- Herdr panes belong to the Herdr server and survive EAK window reloads, workspace switches
  and app restarts (not machine restarts).
- Herdr must be installed or bundled (`externalBin`); packaging is a separate task.
