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
