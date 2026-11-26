# ADR-0054: AI tool adapters (`eak-agents`) and Settings › AI tools
- **Date:** 2026-11-26
- **Context:** Wave-2 slice W2-E (`eak-v2-remap/report.md` §4.1; Traycer review §11.3). Engineers
  must be able to connect Claude Code or Codex to a project without a terminal, use the tools' own
  sign-in, and start sessions, while the Terminal workspace stays unchanged. The earlier product
  call "no launcher anywhere" (`docs/terminal-api.md`) was settled by the captain's v2 intent
  (Traycer call #1: a launch button outside the Terminal workspace).
- **Options considered:**
  1. EAK stores API keys for CLI tools and drives their APIs directly. Rejected: ADR-0017 keeps
     CLI tools on their own sign-in; EAK would become a second agent.
  2. Write each tool's config files (`.mcp.json`, `~/.codex/config.toml`) from EAK. Rejected: the
     formats are the tools' own and change; a file written into the project folder would bypass
     the engine boundary (ADR-0039).
  3. **Chosen:** a new crate `crates/eak-agents` that runs each tool's own documented commands:
     `--version`; its status command for sign-in; `mcp add` / `mcp get` / `mcp remove` to register
     `eak-mcp --project <dir> --agent <tool>`; its own login in a terminal tab; its headless JSON
     stream for background sessions.
- **Decision:**
  - Sign-in is `Option<bool>`: `None` (the tool cannot tell) is shown as "unknown", never "yes".
    EAK reads only `loggedIn`/`authMethod` (Claude) or the exit code (Codex); no credential file.
  - Claude Code registers in its **local scope**, keyed to the project folder and private to the
    user, so nothing is written into the project. Codex has only a user-level config: its
    registration names one project at a time, and Disconnect refuses to remove another project's.
  - A tool whose commands are not confirmed (OpenCode, Gemini CLI today) is found, versioned and
    started in a terminal tab; Connect and headless runs report `Unsupported` with the reason.
  - Terminal sessions and sign-in are new tabs of the existing Terminal workspace through its
    session manager (with `EAK_AGENT_NAME`, `EAK_TASK_ID`); the workspace UI is unchanged.
  - Agent identity for the WEFT registry (`agent:<EAK_AGENT_NAME>@<EAK_SESSION_ID>`): the MCP
    registration carries `EAK_AGENT_NAME` and `EAK_AGENT_TOOL` (`claude-code`, `codex`). A terminal
    tab keeps the `EAK_SESSION_ID` its terminal session exports; a headless session gets its task id
    as `EAK_SESSION_ID`. Claude Code passes its environment on to `eak-mcp`; Codex gives a server
    only its configured env, so each launch adds `-c mcp_servers.eak.env_vars=["EAK_SESSION_ID"]`.
  - Headless sessions are child processes whose output goes to `<app data>/agent-sessions/<id>.log`
    and whose documented stream lines become events. Headless Claude allows only EAK's own MCP tools
    (`--allowedTools mcp__eak`); the harness still bounds them and every design change goes through
    `Engine::execute`.
- **Consequences:** no IR, `Command`, report or harness-envelope change. Nine new IPC commands
  (`agent_*`, including `agent_session_restart`). The adapters' flags are pinned by fake-binary tests; a tool that changes its CLI shows
  "unknown" or an error with the tool's message, not a false state. App-launched sessions run the
  harness in propose mode (`EAK_HARNESS_APPLY_MODE=propose`, ADR-0051; amended 2026-11-26 on the
  captain's instruction): an agent started from EAK proposes and a person applies. A session the
  user starts by hand keeps the harness default. The Agents area (W2-F) may add a per-session choice.
- **Source requirement:** Captain's directive 2026-11-26 (AI Tools Integration); Spec §9 (agents act
  through the same commands as people).
