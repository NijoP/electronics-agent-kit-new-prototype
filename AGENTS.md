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

## Working loop (Ponytail-style method, development-only)
Read the affected code first, make the smallest complete change, reuse existing modules, add a
test (a regression test for every bug fix), and run:
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
