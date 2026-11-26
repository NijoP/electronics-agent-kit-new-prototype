# EAK Agents workspace: UX architecture and interaction spec

Status: **M1.5 (agent-workspace part), M2.1 and M2.2: first vertical slice implemented.**
Sources: execution brief §4.1, §9 and §10; the Traycer study (M1.1, PR #147, §9 "adopt / do not
adopt"); ADR-0051 (proposals), ADR-0053 (WEFT), ADR-0054 (AI tool adapters) and ADR-0058 (this
workspace and durable sessions). Clean-room: no third-party code, icons, artwork or text.

## 1. What the workspace is for

The Agents workspace is the **control room for engineering work done by agents** on one project.
It is not a provider-settings page (that stays in *Settings › Model settings* and *Settings › AI
tools*) and it is not a chat. In one look an engineer must be able to answer:

| Question (brief §9.1) | Where it is answered |
|---|---|
| Which project and revision is active; which revision did each task start from? | Context bar; inspector *Revision* rows ("created at r3 · project now r5") |
| What needs me next? | Roster *Needs you* group, pinned first |
| Which agents exist, what role, what are they doing? | Roster *Agents*; centre surface for the selected agent |
| What was delegated, to whom, why? | Roster *Tasks*; centre task view; inspector *Task contract* |
| Who is blocked or waiting on a decision? | *Needs you*; task status chips; session state chips |
| Which design objects did an agent change? | Activity region (revision rows carry `path:key` objects) |
| Which proposals need review? | *Needs you* (review requests to the engineer); centre *Review proposal* |
| Do checks pass, fail or remain unknown? | Activity region: ERC/DRC errors before → after; "not run" is never 0 |
| How do I stop, restart or inspect a session safely? | Centre session view: Stop / Restart / log, each enabled only when real |

## 2. Concepts kept apart (brief §10.1)

Each concept has its own noun; none stands in for another.

* **Runtime**: a coding-agent CLI installed on this computer (Claude Code, Codex, ...), probed with
  its own commands (`eak-agents`). A found binary is not a connection, and a connection is not a
  session.
* **Session**: one run of a runtime started from EAK (terminal tab or headless process). Its state
  comes from process facts only: `running`, `completed`, `failed`, `stopped`, or `unknown` when EAK
  restarted and no longer supervises it.
* **Agent**: an identity registered on the project's WEFT bus (`agent:<name>@<session>`), with the
  tool, model and capabilities **its launcher declared** (EAK does not verify the model).
* **Task**: a WEFT delegation (owner, creator, dependencies, parent, status).
* **Message**: a typed WEFT message (delegate, context share, status, finding, review request,
  review verdict, blocker).
* **Proposal**: a revision-bound design change (ADR-0051) that only the engineer applies.
* **Revision**: the engine's project revision; every row that has one shows it.

## 3. Layout

```
┌ rail ┬──────────────────────────── context bar ──────────────────────────────────────────┐
│      │ Agents · <project> · r12 · WEFT journal 18 entries · app sessions: propose mode    │
│      ├──────────────┬───────────────────────────────────────────┬─────────────────────────┤
│      │ ROSTER       │ WORK SURFACE                              │ INSPECTOR               │
│      │ [filter  /]  │ Overview (nothing selected): needs-you,   │ task contract / agent   │
│      │ Needs you  2 │   task graph, recent messages             │ card / session facts /  │
│      │ Agents     3 │ Agent: owned tasks, message thread        │ runtime facts; "Not in  │
│      │ Tasks      4 │ Task: dependencies, thread, proposals     │ the task contract yet"  │
│      │ Sessions   1 │ Session: first message, state, events,    │ for the §10.4 fields    │
│      │ Runtimes   4 │   live log, Stop / Restart                │ WEFT does not carry     │
│      │              │ Runtime: facts, Launch session            │                         │
│      │              ├───────────────────────────────────────────┴─────────────────────────┤
│      │              │ ACTIVITY (collapsible): timeline from the project journals,          │
│      │              │ filtered to the selection                                            │
└──────┴──────────────┴──────────────────────────────────────────────────────────────────────┘
```

Evaluation of the brief's §9.2 composition, and what changed:

* **Kept:** context bar, left roster, centre work surface, right inspector, bottom activity.
* **Changed: "Needs you" first.** The most important fact for an engineer is what waits on *them*:
  a review request routed to `engineer`, a blocked task, a failed or unsupervised session. It is a
  roster group pinned above everything, not a dashboard card.
* **Changed: no terminal in the bottom region yet.** The native PTY stays in the Terminal workspace
  (captain rule: unchanged this phase). A terminal session links there with *Show in Terminal*; the
  bottom region is the event timeline. Embedding the PTY is M2.7.
* **Changed: no chat composer.** WEFT messages are typed and agent-to-agent; sending free text to a
  running CLI session needs a per-adapter *send* capability that no adapter declares yet
  (M2.3/M2.4). The centre surface shows the thread; the composer is absent rather than fake.
* **Review is a real surface, not a link.** *Review proposal* opens EAK's proposal review
  (ADR-0051, recomputed on a copy) inside this workspace; Apply there is the engineer's act.

## 4. Interaction rules

1. **Selection.** One selection at a time: overview, agent, task, session or runtime. `Escape`
   returns to the overview, `/` focuses the filter, `↑`/`↓` move between roster rows.
2. **Filter.** One text filter over every roster group (name, id, tool, title, status). Sessions
   have a *This project only* toggle (on by default).
3. **Live state without model polling.** The workspace reads files and process facts only, never a
   model: WEFT state and the activity timeline every 2 s, sessions every 3 s, and only while the
   workspace is shown. Runtimes are probed when it opens and on *Check again*.
4. **Truthful availability (§9.4).** Every control acts, or is disabled with its reason as the
   tooltip and in the text beside it. Examples: *Stop* on a finished session ("The session is not
   running"); *Stop* on an unsupervised session ("Started before EAK last closed; EAK no longer
   supervises its process"); *Launch session* with nothing installed ("Codex is not installed").
5. **Message delivery states.** WEFT records sends but not reads. Each message shows `recorded` (it
   is in the journal) and, where the journal proves it, `answered`: a review request with a verdict
   from its recipient, or a delegation whose owner has set a status. `read` is shown as *not
   tracked*, never guessed (Traycer A5; the WEFT change is M3).
6. **Revision binding.** A task shows the revision it was created at and whether the project has
   moved since; a proposal shows its base revision and *stale* when it can no longer be applied.
7. **Permissions.** Sessions started from EAK run the harness in **propose mode** (ADR-0054): the
   agent creates proposals and cannot apply them. For agents EAK did not start, the inspector says
   their mode was set by whoever started them; EAK does not guess.
8. **Contract fields not recorded yet.** The inspector lists the brief §10.4 fields WEFT tasks do
   not carry yet (acceptance criteria, permission tier, affected objects, timeout policy,
   deliverable type) under *Not in the task contract yet*, with the roadmap goal, instead of
   showing empty fields.

## 5. Durable sessions (M2.2)

Sessions started from EAK are recorded in an append-only index
`<app data>/agent-sessions/sessions.jsonl`: one record per state change, and the last record per id
wins. On start EAK replays it and reconciles:

* a record that was `running` when EAK closed becomes **`unknown`**, with the reason ("EAK closed
  while this session was running; it is not supervised any more and its outcome was not recorded").
  EAK never assumes that a closed app or tab means success (brief §10.5);
* a headless session's log stays readable from its log file;
* a terminal session's output is not kept after EAK closes, and its old tab id is never used to
  read, stop or close a tab of the new run;
* *Restart* starts the same runtime, surface, project and first message as a new session.

## 6. Roadmap mapping

| Capability (brief §9.3) | This slice | Later goal |
|---|---|---|
| Agents entry point separate from settings | **Done**: the rail's *Agents* opens this workspace | — |
| Roster, durable sessions, task list/graph, activity timeline, session detail | **Done** | — |
| Start a session (terminal / headless) | Done, from the runtime view (existing adapters) | M2.3/M2.4 real-CLI tests |
| Stop / restart | Done where the adapter supports it | M2.8 interrupt / resume |
| Review proposals | Done (embedded review dialog) | M3 |
| Chat / send to a session | Not offered (no adapter declares *send*) | M2.3/M2.4 |
| Specialist profiles | Not built; no control shown | M3 |
| Create a task from the UI | Not built: WEFT delegation is agent-to-agent | M3 |
| Embedded PTY | Linked (*Show in Terminal*) | M2.7 |
| Read receipts, reply threads, quiet-peer notice | Shown as not tracked | M3 (WEFT change, ADR) |

## 7. Evidence

* vitest: `src/agents/agentsModel.test.ts` (roster, needs-you, delivery state, task graph, revision
  binding) and `src/agents/agentsWorkspace.test.tsx` (the workspace against a mocked IPC; every
  control acts or says why).
* Rust: `crates/eak-agents/src/sessions.rs` tests (index replay, running → unknown, restored log
  tail, restored stop refused) and `apps/desktop/src-tauri/tests/agents.rs` (`weft_view` from a
  real journal).
* ui-evidence: `apps/desktop/e2e/scenarios/m2-agents.json` (real window, two scripted agents on the
  real WEFT bus).
