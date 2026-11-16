# ADR-0036 Agent task graph contract

Status: accepted. Step 1 of the orchestration order is implemented in source with test source;
nothing here has been compiled or run by the author (CI is the first compile).

Context: `agent_task.rs` records one agent's task and phases. Multi-specialist work needs explicit
structure: what runs when, what each node may read and write, and independent judgement
(`docs/research/WEFT_ORCHESTRATION_AUDIT.md` lists the ideas; no Weft code or text is used).

Decision: `eak-harness/src/task_graph.rs` defines a pure, serialisable `Graph` of `Node`s (role,
typed input/output ports, allowed tools, write scope, timeout, cancellable flag, optional token budget)
and `Edge`s between typed ports. `Graph::validate` runs before any execution and rejects: duplicate
ids or ports, edges to unknown nodes or ports, kind mismatches, inputs with no source or more than one
source, external inputs that are also wired, cycles, zero timeouts, unknown tools (when a tool list is
given), PCB roles/writers that are not downstream of schematic generation and a netlist check, two
nodes writing the same scope without an order, reviewers holding a write scope, and an evaluator that
reads non-evidence kinds (agent narrative) or has no review node upstream. `ready`, `topo_order` and
`descendants` give dependency-ordered, parallel-capable scheduling and the dependent set used for
invalidation. Base revision is a run-time property recorded by the journal (next step), not a graph field.

Consequences: later steps (journal, scoped context, invalidation, approval, role templates, evaluator,
CLI/MCP and desktop views) build on these types. Adding a role or data kind is an additive change.

## Journal (step 2)
`eak-harness/src/task_journal.rs` keeps `agent-journal.jsonl` beside the project: append-only, one
entry per line with run id, project revision, sequence number and a SHA-256 chain over the previous
entry, so edits, deletions and reordering are detected on read. Only an incomplete final line (crash
mid-write) is ever dropped, on the next append. Run state is rebuilt by folding entries, so
interrupted, suspended and completed nodes survive a restart. External effects are gated by an
idempotency key: planned before, committed after; a planned-but-uncommitted key yields
`NeedsReconcile` and is never replayed blindly.

## Scoped context (step 3)
`eak-harness/src/task_context.rs` builds, per node, a `ScopedContext` holding only: the task contract
(role, tools, write scope, timeout, budget, expected outputs), exactly the inputs the node's ports
declare (small values inline, large ones by output reference with a SHA-256 digest and size), and
stable references to the design parts its role may read (`parts_for`), each bound to the run's base
revision. There is deliberately no field for chat history, unrelated results or the whole project; no
role may read every part. An input whose producer has not finished, or an external input that was not
supplied, is refused rather than guessed.
