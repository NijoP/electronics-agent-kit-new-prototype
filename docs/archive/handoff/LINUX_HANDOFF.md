# EAK development handoff: Windows laptop -> Linux laptop (2026-11-13 ~21:00 IST)

Development on the Windows laptop stopped on 2026-11-13 at about 21:00 IST. All agents were shut
down, and every commit that existed was already on GitHub. Development continues on a Linux laptop
under a fresh Firstmate crew. This folder is the complete handoff:

| File | What it is |
|---|---|
| `LINUX_HANDOFF.md` (this file) | State at handoff, unfinished work, blockers, Linux setup, **master prompt** |
| `FINAL_DIRECTIVE_2026-11-14.md` | The owner's governing delivery directive, verbatim. Highest priority. |
| `OPERATING_RULES.md` | Standing project rules: frozen stack, dependency approvals, authority, model rule, method |
| `CREW_CHARTERS.md` | Charter and scope of each persistent secondmate, to recreate the crew |
| `BACKLOGS.md` | Every Firstmate home's work items, with full task notes |
| `SCOPE_AUDIT_2026-11-13.md` | Architecture/scope audit: general-purpose platform vs. the two acceptance projects |
| `DESKTOP_DEPENDENCY_REQUEST.md` | The desktop dependency list awaiting the owner's approval |

## 1. Repository state at handoff

- `main` @ `b6bfb92`: M1 engineering foundation (EAK-IR, typed command engine, engineering, validation,
  manufacturing crates; 35 tests passing), tool-attribution settings, and executable `scripts/*.sh` (PR #2).
- No uncommitted or unpushed code existed in any worktree at shutdown (every local clone and worktree was checked).
  Branches `fm/eak-jlcpcb-prep` and `fm/devdash-mvp` had no commits beyond `main` and were never pushed;
  that work had not started.
- Firstmate's private state (backlogs, charters, rules, reports) is copied into this folder.

## 2. Unfinished work: open pull requests (all MERGEABLE, none merged)

| PR | Branch | Owner | CI at handoff | Next action |
|---|---|---|---|---|
| [#12](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/12) Tauri 2 desktop shell bound to the command engine | `fm/eak-desktop-shell` | eak-desktop | 8/8 green | Needs the owner's approval of `DESKTOP_DEPENDENCY_REQUEST.md`, then merge. Foundation for both editors. |
| [#11](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/11) EAK CLI and three-OS CI matrix fixes | `fm/eak-cli-ci` | eak-core | 4/4 green | Merge. |
| [#9](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/9) Acceptance project A: ESP32-C3 dashboard | `fm/eak-val-esp32` | main firstmate | 4/4 green | Merge; still needs the native editor workflow (directive section 8). |
| [#3](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/3) Intelligence: provider trait, typed tools, approval-gated agent loop | `fm/eak-intelligence-core` | eak-intelligence | 4/4 green | Merge. |
| [#10](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10) Acceptance project B: 12-24 V to 5 V / 3 A buck board | `fm/eak-val-power` | main firstmate | red: jobs not started (billing, see §3) | Re-run CI after billing is fixed; the worker reported no code change needed. |
| [#1](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/1) Ponytail-style development method in AGENTS.md | `fm/eak-dev-method` | main firstmate | red (pre-#2 exec-bit failure) | Rebase on `main`, re-run. |
| [#4](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/4) Per-layer board stackup, EAK-IR schema v2 | `fm/eak-stackup` | eak-electronics | red / queued | Rebase on `main`, re-run. |
| [#5](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/5) Thermal reliefs, silkscreen refdes, DRC-014/015, IPC register | `fm/eak-fab-detail` | eak-electronics | red (pre-#2 exec-bit failure) | Rebase on `main`, re-run. |
| [#6](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/6) Synchronous-buck power calculations, ERC rating checks | `fm/eak-power-calc` | eak-electronics | red (pre-#2 exec-bit failure) | Rebase; check the audit's warning that it narrows the calculation API to the buck project. |
| [#7](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/7) Validation-board parts with datasheet provenance | `fm/eak-lib-expand` | eak-electronics | red (pre-#2 exec-bit failure) | Rebase on `main`, re-run. |
| [#8](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/8) Offline LCSC sourcing records and SOURCING report | `fm/eak-lcsc-sourcing` | eak-electronics | red (pre-#2 exec-bit failure) | Rebase; extend into the full LCSC integration (directive §5). |

Most red PRs failed only because `scripts/*.sh` lacked the executable bit on Linux/macOS, which #2
fixed on `main`; a rebase is expected to turn them green.

## 3. Blockers to clear first

1. **GitHub Actions is not starting jobs.** The latest run (43448326418) reports: "The job was not
   started because recent account payments have failed or your spending limit needs to be increased."
   The owner must fix billing or the spending limit in GitHub settings. Until then, no PR can show a
   verified green result, and nothing may be merged.
2. **Desktop dependency approval.** PR #12 adds `tauri`/`tauri-build` (Rust) plus React, Vite and the
   test tooling (npm); see `DESKTOP_DEPENDENCY_REQUEST.md`. The stack rule requires the owner's explicit
   approval. Both editors depend on it. The 3D packages (`three`, `@react-three/fiber`) are no longer needed
   for this release (3D is deferred).
3. **Unstarted priority work under the directive** (none of it existed in code at handoff):
   - design system and interaction architecture (`ux-ds`, `ux-ia`, eak-ux);
   - editor-enabling typed commands (`eak-editor-cmds`, eak-core);
   - actionable ERC/DRC findings (`eak-actionable-findings`, eak-electronics);
   - interactive schematic editor (`eak-desktop-schematic-edit`) and PCB editor (`eak-desktop-pcb-edit`);
   - full LCSC integration (`eak-lcsc-integration`);
   - JLCPCB prep (`eak-jlcpcb-prep`);
   - universal providers (`universal-provider-system`);
   - release packaging (`eak-release-pkg`, `eak-desktop-mfg-release`);
   - AI assistant dock (`eak-desktop-agent-studio`).
4. **Parked by the directive:** the 3D viewport, the Terminal Center and the Model Provider Center UI;
   the dev dashboard (`devdash-mvp`); the Windows laptop optimizer.

## 4. Windows-only problems that do not carry over

The Windows host needed local Firstmate workarounds: native symlinks, a terminal working-directory patch,
path-form mismatches, and a Windows-only failure of the no-mistakes review step. Defender CPU load and
about 1.5 GB of free RAM also limited the crew to one or two workers. Linux needs none of these
workarounds, so the normal Firstmate delivery path (`no-mistakes-prod-only`) applies again. On Windows,
crew PRs used direct-PR as a temporary workaround.

## 5. Linux setup (once)

1. Install: git, `gh` (authenticated to the account that owns this repo), Rust stable, Node.js LTS
   (build-time only), the Tauri 2 Linux prerequisites (`libwebkit2gtk-4.1-dev build-essential curl wget
   file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`), tmux, and Claude Code.
2. Clone Firstmate (`https://github.com/kunchenguid/firstmate`) to a home directory, e.g. `~/firstmate`,
   and follow its README to install its helpers (`treehouse`, `no-mistakes`, `gh-axi`, `tasks-axi`).
3. Start Claude Code inside `~/firstmate` with model Opus 5.5 (`claude --model claude-opus-5-5`) and paste
   the master prompt below.

## 6. MASTER PROMPT for the Linux Firstmate

Paste this as the first message to the primary Firstmate session on the Linux laptop:

```text
You are the primary Firstmate for Electronics Agent Kit (EAK). Development moved from a Windows laptop
to this Linux laptop on 2026-11-13. Take the helm and continue it.

1. Session start: run bin/fm-session-start.sh and handle its digest.
2. Register the project: load project-management and add EAK as `eak` from
   https://github.com/NijoP/electronics-agent-kit-new-prototype (private) with posture
   [no-mistakes-prod-only +yolo]: EAK merges are delegated to Firstmate, green only, never force-push,
   never bypass required checks, never delete branches, never change repo visibility or account settings.
3. Read the handoff in the EAK repo, in this order: docs/handoff/LINUX_HANDOFF.md,
   docs/handoff/FINAL_DIRECTIVE_2026-11-14.md, 
    docs/handoff/SCOPE_AUDIT_2026-11-13.md,
   then the three source-of-truth PDFs in docs/source-of-truth/.
4. Record the final directive as the owner's preferences in data/captain.md
   (inspect then update). Model rule: agents use only Opus 5.5 (claude-opus-5-5) or Sonnet 5.5
   (claude-sonnet-5-5); write config/crew-dispatch.json routing routine, well-specified work to
   Sonnet 5.5 and everything else to Opus 5.5, and pin secondmates to Opus 5.5.
5. Recreate the crew: load secondmate-provisioning and create persistent secondmates with the ids,
   charters and scopes in CREW_CHARTERS.md: eak-core, eak-electronics, eak-desktop, eak-intelligence,
   eak-ux, eak-clock (deadline monitor; reset its checkpoints to the directive). Do NOT recreate
   eak-dashboard or eak-windows-optimizer unless the owner asks. Hand each secondmate its open items from
   BACKLOGS.md plus the directive work for its domain (directive priority: UI/UX design system ->
   schematic editor -> 2-layer PCB editor -> LCSC -> AI assistant -> JLCPCB prep -> 3-OS packages ->
   both acceptance projects through the app; 3D deferred).
6. Unblock first: tell the owner if GitHub Actions still refuses jobs over billing, and ask the owner once
   for approval of docs/handoff/DESKTOP_DEPENDENCY_REQUEST.md (minus the 3D packages); never add an
   unapproved dependency.
7. Recover the open PRs listed in LINUX_HANDOFF.md section 2: have each owner rebase its red PR on main
   and get a verified green 3-OS result; merge green PRs through bin/fm-pr-merge.sh; then remove
   docs/handoff/ notes that became stale.
8. Keep the rules: frozen stack (Rust core, versioned EAK-IR, one typed command system for human and
   AI, Tauri 2, React+TypeScript, Vite, SQLite only for metadata/index/cache), no Python anywhere,
   Ponytail minimal-code method, no fake functionality, honest maturity states in
   docs/REQUIREMENTS_TRACEABILITY.md, nothing called production- or manufacturing-ready without
   verified evidence. No money, fabrication orders, public releases, private credentials or hardware
   access without the owner's explicit authorisation.
9. Report to the owner only outcomes, decisions needed, blockers and the final delivery report in the
   form the directive's section 11 asks for.
```

## 7. Secondmate launch prompt (if a mate must be relaunched by hand)

Firstmate's `bin/fm-spawn.sh <id> --secondmate` launches each mate with its own charter, so no extra prompt
is normally needed. If a mate session is started manually inside its home, send it:

```text
You are the <id> secondmate of the EAK Firstmate crew. Run bin/fm-session-start.sh, read your charter
(data/charter.md), your backlog, and the EAK repo's docs/handoff/ (FINAL_DIRECTIVE_2026-11-14.md and
OPERATING_RULES.md first). Reconcile and continue only work routed to you by the main firstmate, within
your scope. Report through your parent channel, never to the owner directly.
```
