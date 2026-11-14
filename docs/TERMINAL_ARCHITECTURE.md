# Terminal workspace architecture

EAK's Terminal workspace is the terminal engine in `crates/eak-terminal`, shown in the
desktop app with xterm.js. Its agent-CLI name/alias table (`src/agents.rs`) is derived from
Apache-2.0 third-party source and keeps that source's notices; see
`third_party/licenses/THIRD_PARTY_NOTICES.md`. No third-party binary is bundled or run.

```
EAK desktop (Tauri backend, apps/desktop/src-tauri/src/terminal.rs)
└── eak_terminal::Manager           owns every session for the app's lifetime
    ├── PTY sessions                portable-pty: Unix PTY / Windows ConPTY; reader thread per
    │                               session -> 2 MiB scrollback + `terminal-output` events
    ├── spaces / tabs / panes       a space = a root folder (one per project); tabs hold one or
    │                               more panes (row/column splits); focus; tab reorder
    ├── status                      process first, then agent reports, then labelled heuristics
    ├── saved layout                terminal-sessions.json: spaces, tabs, panes, names, programs
    └── control API + eak-term CLI  token-authenticated loopback JSON API (docs/terminal-api.md)
Frontend (apps/desktop/src/terminal)
├── TerminalWorkspace.tsx           spaces sidebar, tabs, splits, status line, menus, shortcuts
└── XTermView.tsx                   xterm.js per session; instances kept across workspace
                                    switches; rebuilt from scrollback after a window reload
```

## Session lifecycle
- **Shells:** the user's `$SHELL` on Linux/macOS, `%COMSPEC%` on Windows by default; any
  installed shell can be chosen (`/etc/shells`, or `pwsh`/`powershell`/`cmd`/`bash` on `PATH`
  on Windows). Agent CLIs found on the machine (built-in list or `terminal-agents.json`) are
  offered as launchers; launching one opens a shell and types the command.
- **Switching workspaces / reloading the window:** sessions are owned by the backend and keep
  running; views reattach to them.
- **Closing a session:** confirmation when its process is still running, then SIGHUP to the
  process EAK spawned (SIGKILL after a short grace period) and the PTY is closed, so the kernel
  hangs up the terminal's remaining jobs. Only processes EAK spawned are signalled.
- **App exit:** every session is hung up. The layout and session metadata are saved; on the
  next start those sessions are listed as `ended` ("restart to run again"). **Processes never
  survive an app or machine restart** — only the layout and names do.
- **Restart:** stops the process (if any) and starts the same program in the same folder,
  keeping the scrollback with a marker line.

## Status
| State | How it is known |
|---|---|
| idle | shell is the terminal's foreground process group (Unix) |
| running | another foreground job, or the session's own program, is alive |
| waiting_for_input | Linux: kernel reports the foreground process asleep in a terminal read; else a labelled heuristic (output stopped at a question) |
| blocked | an agent reported it through the API |
| completed / failed | exit code 0 / non-zero, signal, or spawn error |
| ended | listed from the previous run; no process |
Windows has no foreground process group: idle there is a labelled heuristic (quiet output).

## Context contract
Sessions export `EAK_SESSION_ID`, `EAK_PROJECT_DIR`, `EAK_PROJECT_ID` (project spaces) and
`EAK_TERMINAL_API_FILE`. `EAK_HARNESS_SOCKET`/`EAK_HARNESS_TOKEN_FILE` are not set yet.

## Not done
- Project file tree inside the Terminal workspace (to reuse the shell team's explorer).
- Detach from / reattach to processes across an app restart (not implemented; stated above).
- Windows and macOS were not run by this work.
