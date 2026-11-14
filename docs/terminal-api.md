# Terminal control API

With Herdr installed (the normal case), use **Herdr's own CLI and socket API** inside any
Terminal-workspace pane: `HERDR_SOCKET_PATH`, `HERDR_PANE_ID` and `HERDR_BIN_PATH` are set
there, and `herdr pane run`, `pane read`, `pane split`, `agent list`, `agent wait`,
`pane report-agent` etc. work as documented by Herdr. See `docs/HERDR_INTEGRATION.md`.

The API below belongs to EAK's **built-in** terminal (used when Herdr is unavailable) and to
the `eak-terminal` engine. It is separate from the engineering harness CLI.

## Transport and authentication
- Newline-delimited JSON over TCP on `127.0.0.1`, ephemeral port.
- The desktop app writes `terminal-api.json` (`{protocol, port, token, pid}`) to its data
  directory (`dev.eak.desktop`), mode `0600` on Unix; on Windows it sits in the user's own
  profile. Every request carries the token.
- Inside a built-in session `EAK_TERMINAL_API_FILE` points at that file.

Request `{"token": "...", "method": "session.run", "params": {"id": 1, "command": "ls"}}` ->
`{"ok": true, "result": ...}` or `{"ok": false, "error": "..."}`.

## Methods
| Method | Params | Result |
|---|---|---|
| `methods` | - | method names |
| `shells.list` | - | installed shells, default first |
| `session.list` | - | `{spaces, tabs, sessions, focused, layout_seq}` |
| `space.create` / `space.rename` / `space.close` | `root`, `name?` / `id`, `name` / `id` | space; close terminates its sessions |
| `tab.move` | `id` (tab), `index` | - |
| `session.create` | `space?`, `cwd?`, `shell?`, `command?` (argv), `name?`, `cols?`, `rows?` | session |
| `session.split` | `id`, `direction` (`row`/`column`), create params | session |
| `session.get` | `id` | session with `status` |
| `session.send_input` | `id`, `text` (exact bytes; `\r` is Enter) | - |
| `session.run` | `id`, `command` | `{since}` output cursor |
| `session.read` | `id`, `lines?` | `{text}` (escape codes removed) |
| `session.wait` | `id`, `until` = `exit`/`idle`/`output`, `pattern?`, `since?`, `timeout_ms?` (max 10 min) | `{met, status}` |
| `session.focus` / `session.rename` / `session.restart` / `session.close` | `id` (+`name`) | - |
| `session.report_state` | `id`, `source`, `state` = `idle`/`working`/`blocked`, `agent?`, `message?` | - |
| `session.release` | `id`, `source` | - |

Status: `{state, source, detail, foreground, agent, exit_code}`. `state` is `running`, `idle`,
`waiting_for_input`, `blocked`, `completed`, `failed` or `ended`; `source` is `process`
(alive/exit code, Unix foreground process group, Linux kernel wait state), `integration`
(an agent's own report) or `heuristic` (output timing/text; never authoritative).
`ended` means the session was open when EAK last exited; its process is gone, restart it.
Pass `since` from `session.run` to `session.wait` so output arriving before the wait counts.

## `eak-term` CLI
`eak-term list | new | split | get | send | run | read | wait | focus | rename | restart |
close | report | release` (`eak-term --help`). Exit code 2 when `wait` times out.

## Extending without core changes
- Agents report their own state with `session.report_state` (built-in) or Herdr's
  `pane report-agent` (Herdr).
- Extra agent CLIs: `terminal-agents.json` in the app config dir,
  `[{"name": "my-agent", "command": "my-agent --flag"}]`; installed ones are recognised in
  status and offered as launchers.

## Session context contract
Every built-in session exports `EAK_SESSION_ID`, `EAK_PROJECT_DIR` (its space root) and, for
project spaces, `EAK_PROJECT_ID`; inherited `HERDR_*` variables are removed. Herdr project
workspaces get `EAK_PROJECT_DIR`/`EAK_PROJECT_ID` in their root shell. `EAK_HARNESS_SOCKET` /
`EAK_HARNESS_TOKEN_FILE` are not set yet.
