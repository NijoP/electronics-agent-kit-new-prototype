# Decisions

ADRs live in `docs/decisions/` (0017 provider catalog, 0018 engineering harness, 0019 terminal workspace were renumbered from clashing 0016 files during integration).

- Terminal is EAK's own PTY engine (`eak-terminal`); no third-party terminal binary is bundled. One derived table is recorded under `third_party/licenses/`.
- Agents act only through the harness (typed commands, `expected_revision`); no parallel design store.
- The chat-style assistant UI is removed; provider/approval infrastructure stays optional.
- The terminal is an app-level workspace registered through `src/workspaces/`, not hard-wired in `App.tsx`.
- Harness endpoint serves every open project, matched by folder, so multi-project windows work.
- Old EDA-only plans and process hand-offs moved to `docs/archive/`; internal crew notes deleted.
