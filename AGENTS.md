# EAK — agent working rules

These rules are subordinate to the three PDFs in `docs/source-of-truth/` (product spec, then
execution instructions, then reference-repository guide). New explicit product-owner
instructions override all of them.

## Non-negotiables
- One canonical EAK-IR (`crates/eak-core`). Never create a second design representation.
- Every design mutation goes through `eak_runtime::Engine::execute` with a typed `Command`.
  UI, CLI and AI use the same commands; no code writes project files directly.
- No Python anywhere (files, scripts, tests, build steps). `scripts/check-no-python.sh` enforces it.
- No EasyEDA/KiCad as engine or source of truth; no mandatory Freerouting.
- Unknown or unimplemented checks are never reported as passing. Verification reports must list
  `not_implemented` and `limitations`. Nothing is ever called "manufacturing-ready".
- AI never self-certifies: engineering numbers are recomputed by `eak-engineering`, and
  verification runs in `eak-validation`.
- Windows, Linux and macOS are equal targets: no platform-specific assumptions outside `cfg` blocks.

## Development method (Ponytail-style, development-only)
Methodology source: https://github.com/DietrichGebert/ponytail (MIT licence), adapted in our own
words; no Ponytail code or text is copied and EAK never depends on it at build or run time.

Principle: the least code that delivers the most reliable functionality. Build less unnecessary
software, but build every necessary engineering capability properly.

Read before code, every task:
1. Understand the requirement; inspect the modules it touches and what they already provide.
2. Pick the smallest complete change, in this order: does it need to exist at all → is it already
   in the codebase → Rust std / Tauri / React platform → an already-installed dependency → a
   one-line change → the minimum new code. Extend working code rather than adding a subsystem.
3. Note affected interfaces and ownership (table below); finish every caller, test and fixture
   the change breaks.
4. Implement, add or update tests that prove the real behaviour, run the validation command,
   fix what fails, then report evidence, what was skipped or unchecked, and remaining risk.
No big plans for small tasks; never spend more effort on dev docs than on the feature.

Rules:
- Bug fix: find every caller, fix the root cause once in shared code, add a regression test.
- No speculative abstractions, config layers, wrappers over good interfaces, or features for
  hypothetical futures. Every addition needs a clear engineering purpose.
- Moved or refactored code keeps its error handling. Refactor only for a concrete improvement.
- Non-trivial logic leaves at least one small test.
- A deliberate known-limit shortcut gets a `// shortcut: <limit>, <when to upgrade>` comment.
- Keep EAK a cohesive Rust-first modular monolith (Tauri 2, React, TypeScript, Rust); never
  change the stack to follow a reference project.
- Prohibited: unneeded microservices, agent frameworks, permanent agents, background services,
  plugin systems, or config layers; duplicate engines, data models, sources of truth or
  algorithm implementations; crates without a meaningful boundary; redundant frontend state;
  dependencies for what std already does.
- Never cut necessary engineering: electrical/IPC calculations, ERC/DRC, connectivity
  validation, geometry and manufacturing constraints, transaction safety, error handling,
  recovery, validation at trust boundaries, security, tests, cross-platform support. Never
  replace deterministic algorithms with model guesses, or call a simplified version complete
  when it misses the real requirement.

Review (own work and others'), in priority order: correct → safe → holds under load → tested →
fast → lean. Each finding names a concrete failure case and the smallest fix. Ask: could it be
simpler, does it duplicate something, is the abstraction or dependency justified, does it add a
failure mode, does it keep the EAK architecture, can another engineer maintain it?

Validation command:
`cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release`.
Update STATUS.md, the traceability table and ADRs when behaviour or decisions change.

## Module ownership
| Area | Crate / path |
|---|---|
| IR, schema, persistence contracts | `eak-core`, `eak-runtime/{command,delta,engine,store}.rs` |
| Electronics engines | `eak-engineering`, `eak-validation`, `eak-manufacturing` |
| Desktop app | `apps/desktop` (when added) |
| Intelligence engine | `eak-intelligence` (when added) |
Shared-contract changes (IR types, `Command`, `CommandResponse`, report types) need an ADR.

## Safety
Never push to anything other than `origin` (NijoP/electronics-agent-kit-new-prototype), never
force-push, never commit secrets, datasheets under copyright, or API keys.
