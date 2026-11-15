# Weft orchestration audit (Milestone 0)

Status: static-review only, not runtime verified. Weft was cloned shallow into a scratch directory
outside the repository, read, and not built or run.

## What was inspected
`WeaveMindAI/weft` `main` @ `61fdacd44b69ef12ca16b30dd533123549e45514` (2026-11-10). GitHub reports the
licence as `NOASSERTION` because it is the custom O'Saasy licence (see legal review).
Opened: `README.md`, `LICENSE`, `docs/src/thinking/sdp.md`, `docs/src/running/the-journal.md`,
`tangle/claude-code/CLAUDE.md` (headings), and the directory layout of `crates/` and `tangle/`.
Not read in depth: `weft-compiler` (about 33k lines incl. `validate.rs`), `weft-engine`, `weft-dispatcher`.

## What Weft actually is
A Rust workspace with a language (`weft-compiler`), engine, dispatcher, durable `weft-journal` and
`weft-task-store`, a node `catalog/`, and per-assistant "Tangle" kits (`tangle/claude-code`, ...).
- **Graph, not chat orchestration.** The compiler checks every connection before a run; values between
  nodes are recorded in a journal (README). Orchestration lives in the graph, not in model tokens.
- **Journal.** Append-only events: run started with a fingerprint of the program shape, node
  kicked/started/completed/failed/skipped/suspended/resumed/cancelled, `NodeStateSaved`, and
  `PortEmitted` (the only event that carries a value, once). Nothing edits an event. A resume runs
  against the suspended program fingerprint, not the edited source (the-journal.md).
- **Durable waits.** A run may suspend for a human or external event and release its worker.
- **Sequential Diffusion Programming.** Start from one real input, inspect the producing node, fix it,
  re-run only what changed; keep cases that taught something and re-run them after later changes
  (sdp.md; verbs `--seed`, `--from`, `--target`, `--before`, `--group`).
- **Tangle specialists + verification** (CLAUDE.md sections "The specialists", "Verification").

## Useful for EAK (to implement independently)
Typed ports and pre-run graph validation; append-only run journal with program-shape fingerprint;
dependency-aware invalidation (re-run only affected downstream); frozen cases re-run after change;
human approval as a suspended node; scoped context per node.

## Do not copy
Any Weft source, node catalogue entries, prompts or docs text (O'Saasy: see legal review); its SaaS
or hosted-runtime infrastructure; its general-purpose language. EAK's graph is domain-specific.

## Proposed EAK design (M5, not started)
Task graph nodes with typed input/output schemas, role, allowed tools and write scope, base revision,
timeout, cancellation; journal rows stored beside the project (revision-bound); idempotency keys for
external effects; an independent evaluator node that reads EAK-IR and validation evidence, never the
agent's narrative. It builds on `Engine::execute` (single mutation path) and `eak-intelligence` tools.
