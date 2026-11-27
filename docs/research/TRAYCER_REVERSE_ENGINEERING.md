# Traycer reverse engineering for EAK (goal M1.1)

Task `eak-traycer-rea`, written 2026-11-26. This is the full text of the deliverable
`docs/research/TRAYCER_REVERSE_ENGINEERING.md` (execution brief §5.3). A copy sits at that path in
the scout worktree. The worktree is discarded at teardown, so this report is the copy that survives.

It builds on the earlier review, `data/eak-traycer-review/report.md`, written at Traycer `15cdf06`
and EAK `bf88f87`. That review's verdict and UI proposals are not repeated here. This document adds:
- the §5.2 concepts it did not cover (messaging, capabilities, session lifecycle, delivery states, auth profiles);
- a check of its claims against newer source;
- a real REA run;
- a mapping onto what EAK has today at `641729a`, which now has proposals, WEFT and AI-tool adapters.

---

## 0. Summary

- **REA was set up and verified, with limits.**
  - REA 6.10.1 installed into the scratchpad with install scripts disabled, so nothing was compiled.
  - It refused the laptop's Node 22.23.2, so I fetched the official portable Node 24.22.3. Its SHA-256 matched nodejs.org's list.
  - A real stdio MCP handshake worked: `initialize` returned protocol `2025-06-18` and `tools/list` returned **263 tools**.
  - Its static Electron analysis of Traycer's open desktop client worked on a narrow target (preload + IPC contracts). On the whole client it timed out on this laptop.
- **Traycer runtime inspection not performed.** Three reasons:
  1. The app needs an Electron GUI plus Traycer's **closed, signed Host binary** and an account sign-in.
  2. Building the client from source means a bun + electron-builder build, which is too heavy for this 8 GB laptop (99 % full disk, about 1 GB free RAM).
  3. Taking the closed Host apart would mean reverse-engineering proprietary code. The brief does not authorise that, so I did not download it.

  Everything below comes from the open MIT source, the docs, and REA's static evidence.
- **What Traycer gets right, in four ideas worth adapting:**
  1. **Capabilities are separate and probed per agent, not inferred.** Being referenceable, having a readable transcript and being able to receive messages are three different capabilities, and "cannot" is a normal state, not an error.
  2. **An honest session state.** Running / sleeping / stopped, with a separate "why it ended" field and `null` meaning "this host cannot know".
  3. **Threads that expect a reply have a backstop.** A request that wants an answer gets a thread id. If the receiver goes quiet, ends its turn, exits, errors or waits on a human, the sender gets a typed inactivity notice.
  4. **Every delivery state is spelled out.** Sending, unconfirmed, queued, can-steer, waiting for a safe point, after-turn, paused, paused after error. "Accepted" is never shown as "delivered".
- **What EAK should not take:**
  - Full access as the default permission (Traycer's default for every new conversation and for CLI/agent-created agents).
  - An LLM judge approving commands.
  - Cross-machine cloud relay.
  - Model routing and shopping between providers.
  - Hard-coding Traycer's 22-provider capability matrix. The brief forbids it, and Traycer itself treats its docs table as product copy, with the real answer coming from the host at runtime.
- **The gap list for EAK is short and concrete** (§9 and §10):
  - WEFT messages have no reply threading, no delivery state and no inactivity notice.
  - WEFT agent cards have no session state.
  - `eak-agents` reports availability but has no per-capability declaration (resume, steer, receive, transcript, fork).

---

## 1. Research method and REA verification

### 1.1 Method

1. **Read the prior review.** I read `data/eak-traycer-review/report.md` in full and treated its claims as hypotheses to check against newer source.
2. **Cloned the sources.** Shallow clones went into the scratchpad:
   - Traycer `traycerai/traycer` at `4ad190d6` (2026-11-25T23:57-08:00), 357 MB;
   - REA `morluto/rea` at `9e3a2bbc` (2026-11-26T16:53Z), 170 MB.
3. **Read the docs.** I read the documentation under `mintlify/`, the source of docs.traycer.ai:
   - `concepts/agent-to-agent`, `panels/agents` (+ `message-queue`, `agent-questions`, `undo-a-turn`), `settings/providers`, `settings/permissions`, `concepts/terminal-agents-vs-terminals`, `concepts/tasks-and-workspace-folders`, `agents-and-models/coding-agents`, `cli/commands`.
4. **Checked the repo docs against the live site.** `curl` of the three URLs the brief names returned HTTP 200. Distinctive section titles from the repo copies appear in the live pages:
   - "Three Separate Capabilities" / "Durability Is Not Availability": 5 hits;
   - "Managed Profiles": 3;
   - "Auto-accept edits": 3.

   The repo docs are therefore the live docs. Confidence: high.
5. **Read the protocol source** (`protocol/src`, 1,036 `.ts` files): session state, inbox, A2A send/fork/create, message delivery, roles, activity, runtime capabilities, permission modes. I also listed every agent/epic/terminal RPC method name.
6. **Ran REA** on Traycer's desktop client (§1.2).
7. **Mapped onto EAK.** I read the EAK modules that now cover the same ground: `eak-harness/src/{weft,proposal,agent_task,task_journal,task_approval}.rs`, `eak-agents/src/lib.rs`, ADR-0054 and `OperationClass`.

**Clean-room.** No Traycer code, icon, artwork or prose goes into EAK. This document describes behaviour and concepts in my own words. UI label names (for example "Approve") are quoted only as facts about the product.

### 1.2 REA verification (what actually ran)

| Step | Command (scratchpad paths shortened to `$S`) | Result | Label |
|---|---|---|---|
| Licence / responsible-use check | read `rea/LICENSE`, `rea/DISCLAIMER.md` | MIT. The disclaimer excludes DRM/licence circumvention, auth bypass and copying proprietary code. Analysing MIT source that the owner publishes is in scope. | OBSERVED, high |
| Install | `npm install --prefix $S/rea-cli rea-agents@6.10.1 --ignore-scripts` | Exit 0 in 1 min with 42 packages, 217 MB on disk, and an `EBADENGINE` warning (Node 22.23.2 < 22.23.4). No native compile. | RUNTIME-OBSERVED, high |
| Run on system Node | `rea --version` | Refused: "REA requires Node.js 22.x (>=22.23.4), 24.x (>=24.22.1), 26.12.1+". | RUNTIME-OBSERVED, high |
| Portable Node | official `node-v24.22.3-linux-x64.tar.xz` | `sha256sum -c` against nodejs.org `SHASUMS256.txt` → OK. Extracted into the scratchpad; nothing installed system-wide. | RUNTIME-OBSERVED, high |
| CLI | `node24 rea.mjs --version` / `--help` | `6.10.1` and the full command list (analyze-javascript-application, capture-electron-scenario, inspect-electron-fuses, …). | RUNTIME-OBSERVED, high |
| **MCP connectivity** | stdio JSON-RPC: `initialize` → `notifications/initialized` → `tools/list` into `node24 rea.mjs mcp` | `serverInfo {name:"rea", version:"6.10.1"}`, `protocolVersion "2025-06-18"`, capabilities `tools/prompts/completions`, **263 tools**. These include `analyze_javascript_application`, `list_electron_targets`, `inspect_electron_page`, `capture_electron_scenario`, `inspect_electron_fuses`, `observe_javascript_runtime`, `trace_javascript_semantics`. | RUNTIME-OBSERVED, high |
| Static analysis, whole desktop client | `rea analyze-javascript-application clients/desktop/src --json`, 300 s timeout | **Cancelled at the timeout after 45 of 613 files**, peak RSS 560 MB. REA also reported that a semantic graph exceeded its result budget. Not usable. | RUNTIME-OBSERVED, high |
| Static analysis, narrow target | same tool on a copy of `electron-preload/` + `ipc-contracts/` (59 non-test files) | Completed. Evidence `ev_e2e0795d…`, authority `shipped-artifact`, confidence `derived`, coverage `partial`. Graph: 738 nodes, 778 edges, 1 context-bridge API with 25 members, 296 IPC operations (all on dynamic channel expressions, so unpaired), 10 renderer listeners. | RUNTIME-OBSERVED (of REA), high; the facts it yields are static, see §2.3 |

**REA limitations reported by REA itself** (kept, not hidden):
- dynamic IPC channels were not resolved;
- 30 Electron-loading sources were not fully examined;
- "missing findings do not establish absence";
- nothing was executed.

---

## 2. Source, build and runtime examined

### 2.1 What is open and what is closed

| Part | Where | Open? | Examined |
|---|---|---|---|
| Desktop shell (Electron main + preload) | `clients/desktop` | MIT | Source + REA static |
| GUI renderer (React) | `clients/gui-app` (9,724 TS/TSX files, 140 MB) | MIT | Spot-read via the prior review; too large for REA here |
| CLI / host installer | `clients/traycer-cli` | MIT | Docs (`mintlify/cli/commands.mdx`) |
| Wire protocol (zod schemas, versioned RPC contracts) | `protocol/src` | MIT | Read directly: the main evidence base |
| **Host** (runs agents, owns sessions, inbox broker, judge, git) | signed binary the CLI downloads | **Closed** | Not examined (see §0) |
| Cloud (sync, relay, sharing, accounts) | hosted service | **Closed** | Not examined |

**Licence.** MIT, "Copyright (c) 2026 Traycer AI" (`LICENSE`). OBSERVED, high.

### 2.2 Build and runtime

- **Build.** `clients/desktop/package.json` builds with `bun` scripts, `vite` for the renderer, native addons and `electron-builder`. That was not attempted on this laptop. OBSERVED (source), high.
- **Runtime.** **Traycer runtime inspection not performed**, for the reasons in §0. No account, cookie, token or credential was touched.

### 2.3 What REA's static evidence shows about the desktop shell

- **The preload exposes one bridge object, `runnerHost`, to the renderer.** Its 25 members cover:
  - auth (`authRedirectUri`, `authnBaseUrl`, `deviceFlow`, `tokenStore`, `hostTokens`);
  - host process control (`hostLifecycle`, `hostManagement`, `hostControllerStatus`, `hostTray`, `localHostCapability`, `selectionAuthority`, `refreshSelectionFleet`);
  - endpoints (`poolBaseUrl`, `relayBaseUrl`);
  - platform services (`platform`, `power`, `fileDrops`, `service`, `traycerCli`, `migration`, `sandboxArtifacts`, `soundtrackRecording`, `soundtrackSourceAvailable`, `initialRoute`, `sentryRendererDsn`).

  OBSERVED (REA static), exact for the member list.
- **The renderer listens for shell events**, among them a local-host change, the local host becoming unresponsive, a device-flow result, an auth callback, a host quit request, a tray command and a foreground notification (`electron-preload/*-bridge.ts`). OBSERVED (REA static), exact.
- **Agent features do not go through Electron IPC (inference).**
  - No agent, chat or task members appear on the bridge.
  - The protocol has its own WebSocket/mux transport (`protocol/src/host-transport/{websocket,mux,stream-session}.ts`).

  The desktop shell is therefore a thin host-and-auth launcher, and every agent feature is RPC from the renderer straight to the Host. INFERRED, medium-high.

  **Pattern for EAK:** keep the Tauri IPC surface for shell concerns and route agent work through one typed endpoint. EAK already does this with the harness endpoint.

---

## 3. UI and interaction map

All rows below are OBSERVED (docs) with high confidence, unless marked.

| Surface | What it is | Key interactions | Evidence |
|---|---|---|---|
| **Task** | Top-level container for one piece of work: agents, terminals, browsers, artifacts, files, diff, PRs | Nine sidebar panels: Agents, Terminals, Browsers, Artifacts, Git Diff, Pull Requests, File Tree, Sharing, Comments | `mintlify/concepts/tasks-and-workspace-folders.mdx` |
| **Agents panel** | A tree of durable agent sessions (formerly "Chats") | `+` opens "Start a new agent" (Chat or Terminal); row menu: new child, rename, archive, share, copy id, delete (optionally nested); filters for order / archived / interface / ownership; search | `panels/agents.mdx` (Creating, Hierarchy, Panel Controls) |
| **Row status** | Icon per agent | Running, background work, needs attention (for example an approval); Terminal agents also show **Asleep** / **Stopped** | `panels/agents.mdx:331` |
| **Composer** | Prompt box | Coding agent/model picker, permissions, thinking effort, fast mode, attach (16 MiB), `@`-mentions, `/` or `$` skills/commands, voice, Send/Stop | `panels/agents.mdx` Composer; `panels/agents/composer.mdx` |
| **Message queue** | Follow-ups while a turn runs | Enter queues; ⌘/Ctrl+Enter steers into the running turn; reorder, edit, delete, pause/resume; per-row delivery labels (§5.3) | `panels/agents/message-queue.mdx` |
| **Question card** | Replaces the composer when the agent asks | Single choice, multiple choice or free text; pager "2 of 3"; Skip; a summary stays in the transcript; **Cross Question** and **A/B Fork** | `panels/agents/agent-questions.mdx` |
| **Approval card** | In the composer area | Approve / Deny; Approve all / Deny all for several requests; a separate file-edit approval card; expandable full command; "Allow from now on…" drafts a rule and saves nothing until you save | `panels/agents.mdx:95` |
| **Work-in-progress stack** | Panels above the composer | **Files changed** (Review all, Undo all, per-file undo); **Active agents** (Stop all); **Background** (sub-agents, commands, monitors, wakes, MCP tools; Stop all, Cancel wake); stopping a parent asks whether to also stop its N sub-agents | `panels/agents.mdx` Work In Progress |
| **Changes card + Undo** | Per assistant turn | Restore files to the start of the turn (later changes to them revert too); skipped/failed counts; owner-only, same host only, not during a turn; artifacts optional | `panels/agents/undo-a-turn.mdx` |
| **Fork** | New agent from a chosen point | Title, harness (may differ), host and workspace; the original is unchanged | `panels/agents.mdx:181` |
| **Agent office** | Messages between agents | Office view (agents at desks; hosts as floors) or Graph view; signals for working / waiting for reply / needs you / queued / idle / failed / unanswered / archived; message kinds Request, Reply, Notice, Created; timeline replay and Follow live | `concepts/agent-to-agent.mdx` Agent Office |
| **Settings › Providers** | One row per coding agent per host | Enabled switch; tabs Account, Profiles & Limits, Usage limits, CLI & Args, Env, Permissions, Model Providers, MCP, Plugins, Skills | `settings/providers.mdx` |
| **Settings › Permissions** | Policy | Tabs: Modes (default for new conversations), Judge, Rules (Environment / Always allow / Ask first / Never allow / Notes), Activity (last 200 judge decisions) | `settings/permissions.mdx` |
| **Plain Terminal** | Shell, not an agent | Host + folder only; no prompt, skills or A2A | `concepts/terminal-agents-vs-terminals.mdx` |

---

## 4. Agent, task and session lifecycle

### 4.1 Entities

| Entity | Meaning | Evidence | Label |
|---|---|---|---|
| Task (wire name `epic`) | Container; owns workspace folders and agents | `tasks-and-workspace-folders.mdx`; RPC names `epic.*` | OBSERVED, high |
| Agent | A durable coding-agent session. Chat and Terminal are **interfaces on the same entity**, not two kinds of thing. | `panels/agents.mdx` intro | OBSERVED, high |
| Interface `gui` / `tui` | Chat: settings can change every turn. Terminal: the launch setup is fixed; the upstream session id is stored so the same session resumes on its original host. | `panels/agents.mdx`; `shared.ts` `surface: z.enum(["gui","tui"])` | OBSERVED, high |
| Plain terminal | A PTY shell; no agent context | `terminal-agents-vs-terminals.mdx`; `terminal.plain.*` RPCs | OBSERVED, high |
| Host | The machine that runs agents and keeps sessions; a user can have several | `concepts/hosts.mdx`; `host-*` contracts | OBSERVED, high |

### 4.2 Session state: the most transferable idea

**`protocol/src/host/agent-session-state.ts:60`** is a closed enum of three values: `running`, `sleeping`, `stopped`.
- **sleeping** covers *every* PTY exit (idle reap, user stop, host restart, the CLI exiting by itself). The agent **resumes** on its next open or message.
- **stopped** is written *only* by record-level operations (archive or delete). Even an archived agent stays addressable: the next message unarchives and wakes it. Only delete is final.
- The field is **nullable, and `null` is an answer.** It means "this host cannot know": a row from a peer host, a cloud replica, or a row older than the field.
- **`:96`** is a separate closed enum for the *last exit reason* (`reaped`, `user-stop`, `restart`, `process-exit`). It is **display only**; all four resume the same way.
- The source comment explains why: treating an idle-reaped session as dead made orchestrators stop addressing live peers.

OBSERVED (source), high.

**For EAK:** the brief's §10.5 list (queued … stopped, timed-out, unavailable) mixes three axes: the task, the session and the delivery. Traycer keeps them apart:
- session liveness (above);
- turn activity (working / waiting / needs-you; the Office signals);
- message delivery (§5.3).

EAK should do the same, and should add `unknown` (null) as a real value instead of guessing.

### 4.3 Create, configure, stop, resume, fork, archive

**From the RPC names** (`agent.create`, `agent.configure`, `agent.stop`, `agent.fork`, `agent.archive`, `agent.list`, `agent.getTranscript`, `agent.tui.prepareLaunch/promptSubmitted/turnEnded/recordActivity`, `epic.createTuiAgent`, `epic.continueSubagent`) and the CLI docs:
- **Create** (`shared.ts:731`, v3). If surface and harness are null, they are inherited from the calling agent. `permissionMode` is nullable. A child is parented to the caller. The CLI default permission is `full_access` (`cli/commands.mdx` Create Options). OBSERVED, high.
- **Stop** ends the current turn only; the agent is not deleted, and a later message wakes it. `--cascade` also stops delegated agents (`cli/commands.mdx:363`). The inbox records who stopped it (user or agent; `inbox.ts`, the `@1.3` notice). OBSERVED, high.
- **Fork** (`shared.ts:1726`) starts from the latest checkpoint only. A fork **does not inherit the source's permission mode** (the mode is a required field). The workspace is inherited unless given. Terminal forks are Claude-only, because other harnesses lack a native session fork. OBSERVED, high.
- **Configure** changes harness / model / profile from the next turn. For Chat interfaces, permission changes apply right away. Switching to Full access mid-turn also approves requests already waiting (`panels/agents.mdx` Permissions). OBSERVED, high.
- **Archive** is refused while the agent works. A message unarchives it. OBSERVED, high.
- **Restart.** A Terminal agent reopened after an app or host restart resumes the upstream session id on its original host. **Durability is not availability:** a Terminal transcript is read from the coding agent's own history, not from scrollback, so it survives the terminal closing. A read fails outright rather than return partial data when the host or that history is missing (`agent-to-agent.mdx:137`). OBSERVED, high.

### 4.4 History

- Task history has search, filter, pin and labels (`concepts/history.mdx`).
- Artifacts have version history with restore (`epic.artifactVersions.list/restore`).
- Per-turn file snapshots back Undo.

OBSERVED, high. How snapshots are stored is in the Host: INFERRED that it is per-host file snapshots, because undo works "only on the host that captured them". Medium.

---

## 5. Collaboration and messaging semantics

### 5.1 Three separate capabilities

`concepts/agent-to-agent.mdx:82`. OBSERVED (docs), high.

| Capability | Requirement |
|---|---|
| **Reference** an agent | Being an agent in the Task. Always available. |
| **Read transcript** | Your own agent on any of your hosts. Another user's *Chat* agent read-only in a shared Task. A Terminal agent only while its host is reachable. |
| **Receive messages** | Your own agent, *with an inbox*: any Chat agent, or Claude Code on the Terminal interface. Codex and OpenCode Terminal agents can send but **cannot receive**, so they cannot ask for a reply, and nobody can *create* one, because it could never receive its brief. |

The hierarchy is provenance only: any agent can reference any other agent, not just its parent or children.

### 5.2 Reply-required and fire-and-forget

- **The send request** (`shared.ts:1526`, `agent.sendMessage` v1.0 to v1.2) carries sender, receiver, epic, prompt, `expectReply` and a nullable `responseId`. v1.1 adds a receiver workspace binding; v1.2 adds image attachments from the sender's own transcript. OBSERVED, high.
- **`expectReply=true`:** the broker registers a pending request, which is idempotent per sender→receiver pair, and returns a **thread id**. The id names the sender's thread, not one message: follow-ups reuse it, and one reply carrying it answers the whole thread. OBSERVED, high.
- **Replies:** a reply is a separate send with `expectReply=false` and that `responseId`. A fresh message does not close the thread. A null `responseId` with no reply expected is a one-shot. OBSERVED, high.
- **Message framing** (`protocol/src/agent/a2a-message-format.ts:34`): the receiver gets a plain-text header naming the sender (title, id, harness) and whether a reply is expected. When one is, the header gives the thread id and the reply tool. The Terminal variant adds a footer pointing to the durable inbox, because terminal notifications can be truncated. OBSERVED, high.
- **Rejected, not queued, when** (`agent-to-agent.mdx:113`):
  - the receiver has no inbox;
  - the receiver belongs to another user;
  - the receiver's host was revoked;
  - the sender wants a reply but has no inbox itself;
  - the message is over 16 MB.

  OBSERVED, high.
- **Cross-host:** delivered through the cloud. The send returns **queued** once the cloud has accepted it, which does *not* confirm delivery. It expires after 24 h and then shows **Not delivered**, and a waiting sender is told. OBSERVED (docs), high. The cloud side is closed: UNVERIFIED in code.

### 5.3 Delivery and queue states

**In the composer** (`message-queue.mdx`): sending to host → delivery not confirmed ("not a failure") → queued for next turn, then one of:
- can steer;
- waiting for provider (the next safe point);
- restart pending;
- steering;
- after turn (this agent cannot take a message mid-turn);
- paused;
- paused after an error.

A **Check delivery** action re-asks the host and does not re-send. Messages from other agents enter the same queue; they can be reordered but not edited. OBSERVED, high.

**On the wire** (`agent/gui/message-delivery.ts:38-46`): a message runs through `pending` → `preparing` → `started` (the commit point) | `withdrawn`. A withdrawn opening hands its text back to the composer, so nothing typed is lost. OBSERVED, high.

**Which agents can steer is per agent.** Several harnesses cannot steer mid-turn (`message-queue.mdx`), and Traycer's own docs table lists this per coding agent (`coding-agents.mdx:16`). EAK must probe this, not copy the table.

### 5.4 Inbox and inactivity backstop

**`protocol/src/host/agent/inbox.ts`:**
- `agent.inbox.subscribe` is a streaming contract used by a `traycer monitor` process that runs inside a Claude Code terminal session.
- The broker queues messages per receiver, in RAM in v1.0. A durable row with `eventId` and `agent.inbox.ack` arrives in v1.2, giving at-least-once delivery. Older monitors get a server-side compatibility ack (`:295`).
- If no monitor is attached, messages queue and replay when one connects.

OBSERVED, high.

**The inactivity notice** (`:123`, reasons at `:167`) is a distinct frame kind, so it never looks like a peer message. The broker sends it to a sender whose receiver stalled. Reasons:

| Reason | Meaning |
|---|---|
| `turn-ended` | Ended its turn with no reply (the accurate, primary signal) |
| `exited` | The process exited |
| `quiet` | A watchdog saw long PTY silence. Advisory only; check the transcript. |
| `user-stopped` | Its turn was stopped; the thread stays open |
| `errored` | Its turn ended on an error, for example a rate limit |
| `awaiting-input` | It is blocked on a human question or approval |
| `receiver-cancelled` | It was stopped outright; the thread is closed and the sender must not re-send |

The notice carries detail text and the list of dropped threads. OBSERVED, high.

**Role-awareness broadcasts go only to connected monitors and are never queued**, so a stale broadcast is never replayed. OBSERVED, high.

### 5.5 Roles

`host/agent/roles.ts:26`:
- An agent can **claim a role over a Task-local scope**, so peers avoid duplicating work.
- Claims **grant no permissions**. They are distinct from the collaborator access list.
- The request names only the caller. The claimant id is an *attribution, not a proof*; the host checks it against the authenticated account.

OBSERVED, high.

### 5.6 Agent selection

Delegation picks the coding agent, model and effort from an "agent selection guide":
- a global guide in Settings;
- a per-workspace override at `.traycer/agent-selection-guide.md`.

Model-list fetches have a configurable timeout (60–180 s). OBSERVED (docs), high.

---

## 6. Permissions and approvals

- **Modes** (`persistence/epic/foundation.ts:234-240`; `settings/permissions.mdx`):

  | Mode | Runs without asking |
  |---|---|
  | Supervised | Reads and searches |
  | Auto-accept edits | Reads, searches, edits |
  | Auto (experimental) | The above, plus commands a judge model approves |
  | Full access | Everything |

  **The default is Full access**, both for new app conversations and for agents created by the CLI or another agent. OBSERVED, high.
- **Some harnesses lack modes**: some run Full access only, and some have no Supervised. Traycer marks the missing modes in the picker. OBSERVED, high.
- **Auto mode:**
  - The judge never refuses on its own; anything it does not approve comes to you, even in a conversation another agent started.
  - Edits to files that define what runs later always ask: git hooks, CI, manifests and build config, editor and agent settings, SSH config, Traycer's own policy files (`settings/permissions.mdx:27`).
  - A provider's own classifier can replace the judge, but then the user's rules do not apply.

  OBSERVED, high.
- **Rules** have five sections: Environment / Always allow / Ask first / Never allow / Notes.
  - **Never allow** is a hard boundary: the judge never approves it, but a person still can, on the card.
  - Repository rules (`.traycer/auto-policy.md`) **can only add restrictions**, never allow something.
  - The last 200 judge decisions are kept per host and are not synced.

  OBSERVED, high.
- **Approval cards** are bound to the human in the conversation. "Allow from now on" drafts a rule and saves nothing until the user saves. OBSERVED, high.
- **Inference:** the permission mode is a property of the agent session, enforced by the Host's per-harness adapter, not by the coding agent. Basis: forks must state their own mode, and the docs mark modes as unsupported per harness. Medium-high.

---

## 7. Provider and auth model

**Three things are kept apart:**
1. the **coding agent** (harness id; 22 provider ids in `provider-ids.ts`, per the prior review);
2. the **account/profile** it runs as;
3. the **model**.

OBSERVED, high.

**Account kinds** (`settings/providers.mdx:81-148`):
- **Terminal account.** The tool's own CLI sign-in, the one you would use in a shell.
- **Managed profiles.** Extra accounts for Claude Code, Codex, Grok and Antigravity. Each has a name, colour and switch, plus a **per-provider rule for what is shared with the Terminal account**:

  | Provider | Shared with the Terminal account | Kept separate |
  |---|---|---|
  | Claude Code | History and settings | The sign-in |
  | Codex | Sessions and skills | Auth and its own config copy |
  | Grok | Nothing | Everything: a separate home directory; ambient keys are not inherited |

  A profile is picked per turn.
- **API-key providers** have three states: Not set / Key set (encrypted on the host, overrides the shell) / From environment. The key itself never crosses the API; only `{supported, configured, source}` does (prior review, `provider-schemas.ts`).

OBSERVED, high.

**Sign-in:**
- The composer shows a banner when a provider is signed out. Depending on the tool it offers:
  - **Authenticate** (opens the browser; paste a code if the browser does not return);
  - a terminal sign-in;
  - "paste an API key or token";
  - **Check sign-in status**.
- Browser sign-in runs on the host's machine; for a remote host, use that tool's CLI there.
- The desktop bridge carries `deviceFlow` and `authCallback` events (REA, §2.3).

OBSERVED, high.

**CLI discovery** (`settings/providers.mdx:149`):
- the binary can be Traycer-managed (versions downloaded, pinned, auto-updated), found on `PATH`, or a custom path;
- per-provider launch arguments for the Terminal interface;
- per-provider env set/unset;
- "Refresh all providers" re-checks sign-in and CLI status;
- providers other than Claude Code and Codex turn on automatically only after the host verifies their credentials.

OBSERVED, high.

**MCP, skills and plugins are per provider**, with a scope of all workspaces on the host or one workspace. A "native subagents" switch makes a provider delegate through Traycer agents instead. Where the tool cannot disable its own subagents, this is only an instruction, *not a tool-level restriction*, and the docs say so. OBSERVED, high.

---

## 8. Directly observed facts and inference

| Claim | Label | Confidence | Evidence |
|---|---|---|---|
| REA 6.10.1 MCP server answers initialize and lists 263 tools | RUNTIME-OBSERVED | high | §1.2 handshake |
| REA static analysis of the whole desktop client is not feasible on this laptop in 300 s | RUNTIME-OBSERVED | high | 45/613 files, cancelled |
| The desktop preload exposes one bridge (`runnerHost`, 25 members) for auth and host control | OBSERVED (REA static) | high (member list exact; coverage partial) | evidence `ev_e2e0795d…` |
| Agent features use Host RPC over WebSocket, not Electron IPC | INFERRED | medium-high | §2.3 |
| docs.traycer.ai = repo `mintlify/` | RUNTIME-OBSERVED (HTTP 200 + title match) | high | §1.1 step 4 |
| Session state running / sleeping / stopped; null = unknown; exit reason is display only | OBSERVED (source) | high | `agent-session-state.ts:60,96` |
| Reply threads, broker inactivity notices with 7 reasons | OBSERVED (source) | high | `inbox.ts:123,167`; `shared.ts:1526` |
| Durable inbox with ack (at-least-once) from inbox v1.2 | OBSERVED (source) | high | `inbox.ts:100-115,291-300` |
| Cross-host messages expire after 24 h | OBSERVED (docs only) | medium (cloud closed) | `agent-to-agent.mdx:118` |
| Full access is the default permission | OBSERVED (docs) | high | `settings/permissions.mdx` Modes; `cli/commands.mdx:341,397` |
| The judge and the permission enforcement run in the Host per harness adapter | INFERRED | medium-high | §6 |
| Undo snapshots are per host | INFERRED | medium | "only on the host that captured them" |
| How the Host drives each CLI (SDK, stream-JSON, ACP) | UNKNOWN | — | Host closed |
| The judge's prompt, model and latency | UNKNOWN | — | Host closed |

**Prior review re-checked** at `4ad190d`, against its `15cdf06`:
- **Confirmed:**
  - the A2A tool names (`traycer_send_message`, host-owned A2A MCP catalog);
  - the permission enum location (`foundation.ts:234`);
  - "fork does not inherit permission mode";
  - the Host is closed.
- **Corrected:** the A2A *transport* is not chat-only. Terminal agents receive through a monitor process subscribed to `agent.inbox.subscribe`, and the durable inbox and ack are newer than that review.

---

## 9. Useful EAK patterns and deliberate non-adoptions

### 9.1 What EAK already has at `641729a`

These map directly onto Traycer concepts:
- **WEFT** (`crates/eak-harness/src/weft.rs`, ADR-0053) is an append-only journal (`weft/journal.jsonl`) of `AgentRegistered`, `TaskCreated`, `TaskStatus` and `Message` events (`:106`). Message kinds are delegate, context share, status, finding, review request, review verdict and blocker (`:71`). Task status is pending, in progress, blocked, done or rejected (`:48`).
- **Proposals** (`proposal.rs`, ADR-0051): propose mode never executes a design command.
- **Grounded task phases** (`agent_task.rs:34`).
- **A hash-chained task journal** (`task_journal.rs`).
- **Approval as a suspended node** (`task_approval.rs`).
- **`eak-agents`** (ADR-0054): `ToolStatus` with `signed_in: Option<bool>`, where None shows as "unknown" (`lib.rs:57`), and `Unavailable::{MissingBinary, NotSignedIn, Unsupported(reason), Other}` (`:47`).
- **`OperationClass`** (`eak-runtime/src/command.rs:637`).

### 9.2 Adopt (re-implemented in EAK's own types)

| # | Traycer pattern | EAK adaptation | Lands in | Needs ADR? |
|---|---|---|---|---|
| A1 | Reference / transcript / receive as separate capabilities | Per-agent `capabilities` on the WEFT `AgentCard`: `can_receive`, `transcript: {available, reason}`, `can_steer`, `can_resume`, `can_fork`. Each is **probed or declared by the adapter at runtime**, never taken from a table. A send to an agent without `can_receive` is **rejected**, not queued. | `weft.rs`, `eak-agents` adapters | yes (WEFT event shape, amends ADR-0053) |
| A2 | Session state with null = unknown, plus a separate exit reason | `session: Option<SessionState{Running, Sleeping, Ended}>` + `last_exit: Option<{Reaped, UserStop, Restart, ProcessExit}>` per agent. Reconcile on app start against the PTY manager / child process (brief §10.5). Never infer "done" from a closed terminal. | `eak-agents/sessions.rs`, `eak-terminal` | yes |
| A3 | `expect_reply` + thread id; one reply closes the thread | `Message { expects_reply: bool, thread_id: Option<String> }`. WEFT `review_request` → `review_verdict` becomes a thread. | `weft.rs` | yes |
| A4 | Typed inactivity notice | An `agent.message.expired` / `notice` event in the brief's §10.6 ledger, with reasons turn_ended / exited / quiet (advisory) / stopped / errored / awaiting_input / cancelled. It is emitted by EAK from process facts and task-ledger phases, **not by a model polling** (brief §10.5). | `weft.rs` + `agent_task.rs` | yes |
| A5 | Delivery states, "accepted ≠ delivered" | Message delivery `queued → delivered → read` (a terminal agent reads its inbox through an `eak` MCP tool, which acks). Show "not confirmed" honestly. | `weft.rs`, UI activity view | yes |
| A6 | A fork must choose its own permission | Any child task or forked session **states its permission tier explicitly** and never inherits a broader one. The default is the brief's tier 3 (create a proposal, cannot apply). | harness context, `task_context.rs` | amends ADR-0051/0053 |
| A7 | "Files that define what runs later always ask" | EAK equivalents always require a click: rule config (`SetRuleConfig`), stack-up, fabricator profile, approval/WEFT/agent config files, release gates. Map them to `OperationClass::EngineeringImpacting` or above. | `command.rs` classification review | maybe |
| A8 | Repository rules can only restrict | A project-local policy file may only tighten EAK's permission tier, never widen it. | harness policy | yes |
| A9 | Role claims grant no permissions | WEFT roles (for example "power-integrity reviewer") stay advisory. Authority stays in EAK's permission tier and the approval node. | `weft.rs` | no (if advisory) |
| A10 | Terminal account vs managed profiles; a per-provider sharing rule | In Settings › AI tools, show **which auth route is active** (CLI sign-in vs API key) per tool, as brief §10.2 asks. Defer multi-profile. | `eak-agents`, settings UI | no |
| A11 | Withdrawn messages return to the composer | If an app-started session fails before it starts, the engineer's prompt returns to the co-pilot box. | desktop UI | no |
| A12 | Inbox footer for truncated terminal notices | Messages injected into a terminal agent end with "read in full with `eak weft inbox`". | `weft.rs`, `eak` CLI | no |

### 9.3 Deliberate non-adoptions

| Traycer feature | Why not |
|---|---|
| Full access as the default; CLI/agent-created agents default to full access | Contradicts brief §10.7 and AGENTS.md ("AI never self-certifies"). EAK's default is proposal-only. |
| An LLM judge approving commands | EAK approvals are rule-based (`OperationClass`) or human. A model never approves. |
| Switching to full access mid-turn auto-approves waiting requests | An approval must be an explicit act per request in EAK (`task_approval.rs`). |
| Cloud relay between hosts, 24 h expiry, the shared-task cloud | EAK is local-first and single-project. Out of scope. |
| Model routing, fallbacks, usage-limit switching | Developer concern; not part of an engineering sign-off. |
| Hard-coding the 22-provider capability matrix | The brief forbids it, and Traycer itself defers to runtime adapter answers. |
| Agent office "people at desks" visualisation | Decorative. A plain graph and timeline of WEFT events serves the audit need. |
| A/B forks in new git worktrees | Parallel copies of one board collide with the single IR. EAK's proposal-on-a-copy already gives safe alternatives. |
| Hidden native-subagent suppression by instruction only | If EAK cannot enforce a restriction at the tool level, it must say so (Traycer does) and not count it as a control. |

---

## 10. Gaps for future investigation

1. **Host internals (closed).** How each CLI is driven (SDK / stream-JSON / ACP), the judge prompt, the snapshot store, and the inbox broker's watchdog thresholds. Only a licensed, authorised inspection could answer these. Not planned.
2. **Traycer runtime UX.** Install the released desktop app on a machine with more RAM and disk, with a test account, and confirm the question-card, approval-card and delivery-label behaviour with REA's `capture_electron_scenario`. That must be the user's own account; no credentials are extracted.
3. **REA on the whole GUI renderer.** 9.7 k files; needs a bigger machine or REA's `source_paths` selection of first-party files. It would map renderer components to RPC calls.
4. **Live probes of each EAK adapter's capabilities** (A1/A2) against real installed CLIs. Which can resume in place, steer mid-turn or fork, and their exit semantics. Fake-binary tests alone do not prove these (brief §10.3).
5. **Terminal inbox delivery for EAK.** Whether Claude Code / Codex terminal sessions can receive an injected WEFT message safely (Traycer uses a monitor process inside the Claude Code session; Codex/OpenCode terminal agents cannot receive). Needs a spike.
6. **Event ledger convergence.** WEFT's journal, `agent-journal.jsonl` and `agent-tasks.json` are three logs. Brief §10.6 wants one versioned ledger. This needs an ADR on whether WEFT becomes that ledger.

---

## 11. What should ship

If promoted, two follow-ups with clear scope:

1. **Doc (S).** Commit this document as `docs/research/TRAYCER_REVERSE_ENGINEERING.md` and link it from the roadmap's M1.1 row. M1.1 can take its `*` only with that PR's CI evidence (brief §2.2).
2. **Code (M, needs an ADR amending ADR-0053).** WEFT capability, session and threading fields: A1–A5 and A12.
   - Acceptance:
     - sending to a non-receiving agent is rejected with a reason;
     - an `expects_reply` message whose receiver ends its turn produces a `turn_ended` notice to the sender;
     - a closed terminal shows `sleeping`, not `done`;
     - an unknown state shows "unknown";
     - regression tests in `crates/eak-harness/tests`.

No captain decisions arise from this report beyond those already registered by `eak-traycer-review`.

---

## 12. Commands and evidence index

Scratchpad: `/tmp/claude-1000/-home-dev--treehouse-eak-9b69fd-4-eak/ec15d109-0eb3-4085-87b9-1451f5c43c42/scratchpad`. It is discarded at teardown.

- Clones:
  - `git clone --depth 1 https://github.com/traycerai/traycer` → `4ad190d6c84ccb38bff9f9947ac39465f2c98d6c`;
  - `git clone --depth 1 https://github.com/morluto/rea` → `9e3a2bbc09872eba06fc4bd09b0547762d385e06`.
- REA:
  - install log: `rea-install.log`;
  - MCP transcript: `rea-mcp.out` (355 KB JSON-RPC);
  - cancelled full run: `rea-desktop.err`;
  - completed narrow run: `rea-preload.json` (19 MB evidence), `rea-preload.err`.
- Live docs: `live-concepts_agent-to-agent.html`, `live-panels_agents.html`, `live-settings_providers.html`.
- EAK: worktree at `641729a` (= `origin/main`, merge of PR https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/144).
- Nothing was pushed. No PR was opened. No Traycer code was copied. No credentials were touched.
