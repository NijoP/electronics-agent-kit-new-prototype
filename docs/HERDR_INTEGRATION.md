# Herdr integration

EAK's Terminal workspace runs the **actual Herdr** (https://github.com/herdrdev/herdr,
Apache-2.0) as a supervised native sidecar. It is not a lookalike: the spaces sidebar, tabs,
splits, pane resizing, keyboard/mouse navigation, agent detection and blocked-state detection,
session persistence, detach/reattach, worktrees, socket API, CLI and plugins the user sees are
Herdr's own.

## Versions
| What | Version |
|---|---|
| Herdr source studied | `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde` (0.9.5, 2026-10-29) |
| Herdr binary verified on the dev machine | 0.7.4 (protocol 16) |
| CLI subset EAK uses | `status server`, `server`, `workspace list/create/focus`, `pane list`, `agent list`, interactive client `herdr --session <name>` — present in both |

## Decision: sidecar, not library
Evidence from the source:
- Herdr is a **binary crate only** (`Cargo.toml` has no `[lib]`; `src/main.rs`), ~290k lines
  whose internal modules (`app`, `server`, `pane`, `pty`, `detect`) are not a public API.
- It vendors `libghostty-vt` (Zig-built terminal parser) and a patched `portable-pty`
  (`vendor/`); linking it into `eak-desktop` would pull a second PTY stack and a Zig build into
  every EAK build on Windows, macOS and Linux.
- It already has the supervised-service shape EAK needs: `herdr server` runs the headless
  server in the foreground, every operation is available through its socket API/CLI with JSON
  replies, and named sessions (`--session <name>`) isolate servers.
- It ships for Linux, macOS and Windows (Windows installer and beta notes in its README), so
  the sidecar keeps EAK's three-platform rule without platform code in EAK.

Copying Herdr's source into EAK was rejected: it would be an unmaintained fork.

## How it works
```
EAK desktop (Tauri backend)
├── herdr::Supervisor   status server → start `herdr server --session S` if missing,
│                       re-check every 5 s, restart with backoff, stop only a server it started
├── eak-terminal PTY    runs Herdr's real client `herdr --session S` in a PTY; bytes stream to
│   bridge              xterm.js in the Terminal workspace (resize, input, scrollback, reattach)
└── herdr::Herdr        CLI/socket calls for project spaces (`workspace create --cwd --label
                        --env`) and agent status (`agent list`)
```
- **Process ownership stays in Herdr.** Panes are children of the Herdr server, not of EAK.
- **Session name:** `eak-desktop` by default, or `EAK_HERDR_SESSION`. `default` is refused.
  Every call appends `--session <name>`, and inherited `HERDR_SOCKET_PATH`, `HERDR_PANE_ID`,
  `HERDR_TAB_ID`, `HERDR_WORKSPACE_ID`, `HERDR_SESSION`, `HERDR_ENV`, `HERDR_BIN_PATH` are
  removed, so EAK started from inside another Herdr can never address that Herdr.
- **Binary:** `EAK_HERDR_BIN`, else `herdr` on `PATH` or in common user install folders. If
  Herdr is not installed, the Terminal workspace says so (with the install link) and offers
  EAK's built-in PTY terminal instead.
- **Project context:** "Open project space" makes (or focuses) a Herdr workspace labelled with
  the project name, rooted at the project folder, whose root shell gets `EAK_PROJECT_DIR` and
  `EAK_PROJECT_ID`. Each project gets its own workspace; workspaces never share a cwd.
- **Persistence:** Herdr keeps panes running while no client is attached. Switching EAK to
  the EDA workspace, reloading the window, or closing EAK leaves the server and its processes
  running (unless EAK itself started the server and `EAK_HERDR_STOP_ON_EXIT=1` is set); the
  next attach shows them again. A machine restart ends all processes; Herdr then restores
  layouts and (for agents that report a resume command) resumes agent sessions.

## What EAK still implements itself, and why
| EAK piece | Why Herdr cannot provide it |
|---|---|
| `eak-terminal` PTY bridge | Herdr's client is a TUI that needs a real terminal; a webview has none, so EAK hosts it in a PTY and renders it with xterm.js. |
| `herdr::Supervisor` | Starting/health-checking a sidecar is the embedding app's job. |
| Built-in sessions + `eak-term` API (`docs/terminal-api.md`) | Fallback when Herdr is not installed; not used when Herdr is available. |

## Known limits
- `EAK_SESSION_ID` is not set per Herdr pane (panes created in Herdr's UI do not go through
  EAK); use Herdr's `HERDR_PANE_ID`. `EAK_HARNESS_SOCKET`/`EAK_HARNESS_TOKEN_FILE` are not set yet
  (the harness socket has not landed).
- Panes created later inside Herdr inherit Herdr's own cwd/env policy (`terminal.new_cwd`),
  not EAK's.
- Herdr is not bundled in EAK installers yet; it must be installed separately.

## Testing on a machine that runs Herdr for other work
Never use the `default` session (or whatever session another tool uses). All automated runs
used an isolated lab session created and torn down by the fleet's `fm-herdr-lab.sh` helper and
set `EAK_HERDR_SESSION` to it.

## Usable inside EAK, not just launched
Herdr's interactive client runs in a PTY owned by EAK and is drawn by xterm.js in EAK's
Terminal workspace, filling the main area. Keyboard, mouse, resize, copy/paste and search go
through EAK's terminal view; no external terminal window or separate app is involved.

## Pinned upstream and modifications
- Pinned upstream for bundling: herdrdev/herdr `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde`
  (0.9.5). The CLI subset above was verified against the released 0.7.4 binary; protocol
  compatibility is checked at runtime (`status server`).
- Modifications to Herdr: **none**. EAK runs the unmodified upstream binary.

## Upstream platform support (from Herdr's README, docs and `distribution/latest.json`)
| OS | Upstream release artifact | Status upstream |
|---|---|---|
| Linux x86_64 / aarch64 | `herdr-linux-x86_64`, `herdr-linux-aarch64` | supported (verified here on Linux x86_64) |
| macOS x86_64 / aarch64 | `herdr-macos-x86_64`, `herdr-macos-aarch64` | supported (not run here) |
| Windows x86_64 | `herdr-windows-x86_64.zip` | "native Windows support is generally available" via ConPTY, with documented platform differences (`docs/windows-beta`); no Windows aarch64 artifact (not run here) |

## Packaging (for the release worker)
- Process boundary chosen, so bundle Herdr as a Tauri `externalBin` sidecar per target triple
  (`binaries/herdr-<target-triple>[.exe]`) built from, or downloaded for, the pinned commit,
  and ship Herdr's `LICENSE` (Apache-2.0) next to it.
- At run time EAK looks for `EAK_HERDR_BIN`, then `herdr` on `PATH`/user install folders. When
  the sidecar is bundled, point `EAK_HERDR_BIN` at it (or extend `terminal::locate` to check the
  sidecar path first) — not done yet in this branch.
