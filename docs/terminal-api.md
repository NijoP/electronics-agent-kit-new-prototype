# Terminal control API

EAK's terminal sessions are controlled through this API (and the `eak-term` CLI). It is
separate from the engineering harness CLI.

## Transport and authentication
- Newline-delimited JSON over TCP on `127.0.0.1`, ephemeral port.
- The desktop app writes `terminal-api.json` (`{protocol, port, token, pid}`) to its data
  directory (`dev.eak.desktop`), mode `0600` on Unix; on Windows it sits in the user's own
  profile. Every request carries the token.
- Inside every EAK terminal session `EAK_TERMINAL_API_FILE` points at that file.

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
| `tab.rename` | `id` (tab), `name` (empty restores the default) | - |
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
- Agents report their own state with `session.report_state` and hand back with `session.release`.
- Extra agent CLIs: `terminal-agents.json` in the app config dir,
  `[{"name": "my-agent", "command": "my-agent --flag"}]`; installed ones are recognised in
  status (the desktop UI no longer shows launcher buttons; run the CLI yourself in a terminal).
- Settings › AI tools (ADR-0053) can open Claude Code, Codex, OpenCode or Gemini CLI, or their
  own sign-in, as a new tab here; such sessions also get `EAK_AGENT_NAME` and `EAK_TASK_ID`. The
  Terminal workspace itself still has no launcher buttons.

## Session context contract
Every session exports `EAK_SESSION_ID`, `EAK_PROJECT_DIR` (its space root) and, for project
spaces, `EAK_PROJECT_ID`. `EAK_HARNESS_SOCKET` / `EAK_HARNESS_TOKEN_FILE` are not set yet.

## Session environment

Each terminal session starts from an explicit allowlist, not the full launching environment:
`PATH`, `HOME`, `USER`/`USERNAME`/`LOGNAME`, `SHELL`, `TERM`, `LANG`/`LC_*`, `TZ`, temp dirs, display and
session variables (`DISPLAY`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `SSH_AUTH_SOCK`), `XDG_*`,
the Windows system variables, and `EAK_*`. Variables that look like credentials (`*_TOKEN`, `*_KEY`,
`*_SECRET`, `*PASSWORD*`, ...) are dropped even under an allowed prefix. Agent CLIs keep their native
auth because their config files live under `HOME`.

Opt-in passthrough: set `EAK_TERMINAL_PASSTHROUGH_ENV=NAME1,NAME2` in the environment that launches EAK
to pass those named variables to every session. Variables given explicitly in a session spec or by the
app are always applied.

Installed CLI: when an `eak` binary sits next to the app executable (Linux/Windows source install) or in
`~/.local/bin` (launchers written by the installer), its directory is put first on the session `PATH` and
`EAK_CLI` points at it, so agents can run `eak` / `eak-mcp`. The source installers build both binaries
(`cargo build --release --locked -p eak-harness --bin eak --bin eak-mcp`), install them, record `cli=` in the
install record, and remove them (and the Windows user-PATH entry) on uninstall.
