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
├── TerminalWorkspace.tsx           Spaces + Agents sidebar, tab strip, pane headers, details
│                                   panel (hidden by default), status bar, menus, shortcuts
├── agents.ts                       Agents-list logic (detected agents only, state mapping)
└── XTermView.tsx                   xterm.js per session; instances kept across workspace
                                    switches; rebuilt from scrollback after a window reload
```

## Session lifecycle
- **Shells:** the user's `$SHELL` on Linux/macOS, `%COMSPEC%` on Windows by default; any
  installed shell can be chosen (`/etc/shells`, or `pwsh`/`powershell`/`cmd`/`bash` on `PATH`
  on Windows). The default action (the tab strip "+", Ctrl+Shift+T) opens a normal shell. EAK
  has no agent launcher: the user types the CLI they installed themselves. EAK never bundles,
  installs, configures or authenticates an agent.
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

## Workspace layout (desktop)
```
┌ Spaces ───────────┬ tab strip: [tab] [tab] ... [+] [▾] [Details] ────────┐
│ ● Board  project  ├──────────────────────────────┬───────────────────────┤
│ ● Power           │ pane header: ● name chip ⇥ ⇩ ✎ ↻ ×                   │ Details (optional,
├ Agents ───────────┤ xterm.js                      │ pane header ...      │ hidden by default)
│ ● claude  Board / │                               │                       │
│   Review / pane 1 ├───────────────────────────────┴───────────────────────┤
└───────────────────┴ status bar: state · evidence · cwd · pid · space · tab ─┘
```
- **Spaces** (sidebar): create, rename (double-click), select and close. A project opened in
  EAK has a natural space (the top bar "+" opens it). Each row shows the folder, a
  most-urgent-state dot and the session count.
- **Agents** (sidebar): aggregated across spaces. A session is listed only when the backend
  recognised an agent CLI as its foreground program from the executable name (alias table in
  `crates/eak-terminal/src/agents.rs`, plus `terminal-agents.json`). Each row shows the agent
  name, space / tab / pane and a state; clicking it focuses that pane. A plain shell is never
  listed as an agent.
- **Tabs**: create ("+" opens a normal shell; "▾" picks a shell), rename (double-click or
  context menu; persisted in the layout file), close, reorder (drag or context menu) and a
  context menu with split and move. Each tab holds one or more panes (row or column split).
- **Panes**: each has a header (status dot, name, detected agent chip, state, split, rename,
  restart, close). The divider between panes is draggable.
- **Details panel**: optional, hidden by default (Details button, Ctrl+Shift+P); shows the
  focused session's status evidence, program, folder and process id.

### Agent states and their limits
| Shown | From |
|---|---|
| working | the session's process is running a foreground job |
| blocked | the kernel/PTY says it waits for input, or the agent reported it via the API |
| idle | the shell is the terminal's foreground process (Unix) |
| done | the process exited (0 or non-zero) |
| unknown | restored from a previous run, or only a quiet-output heuristic (Windows idle) |
Detection is passive and name-based: an agent run through a wrapper script, an alias or under
another executable name is shown as an ordinary terminal. EAK never reads agent screens to
guess, and never claims a shell is an agent. These states describe the *process*, not whether
the agent's task succeeded.

### Keyboard shortcuts (Terminal workspace)
| Keys | Action |
|---|---|
| Ctrl+Shift+T | New tab with a normal shell |
| Ctrl+Shift+W | Close the focused pane (asks first if its process is running) |
| Ctrl+Shift+D / Ctrl+Shift+E | Split the focused pane right / down |
| Ctrl+Shift+R | Rename the current tab |
| Ctrl+Shift+N | New space |
| Ctrl+Shift+P | Show or hide the details panel |
| Ctrl+Shift+A | Go to a session that needs input |
| Ctrl+Shift+1..9 | Select the nth tab of the space |
| Ctrl+PageUp / Ctrl+PageDown | Previous / next tab |
| Ctrl+Shift+Arrow keys | Previous / next pane in the tab |
| Ctrl+Shift+C / Ctrl+Shift+V / Ctrl+Shift+F | Copy / paste / search output (plain Ctrl+C stays an interrupt) |
