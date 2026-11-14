# ADR-0024: Canonical state, component identity, knowledge retrieval and the target architecture

Status: accepted (2026-11-15). Code-reviewed only; nothing compiled or run.
Sources: captain directive part 2 §7, §8, §11, Step 4; directive §2.6.

## Context
EAK must stay one engineering kernel with optional agents, grow multi-domain and component-aware, and
get grounded knowledge retrieval without a RAG platform. This ADR fixes the target in one place so
parallel workers extend it consistently. "State" says what exists today.

## Decision (Step 4 list)

| Topic | Decision | Depends on | State |
|---|---|---|---|
| Canonical state ownership | EAK-IR in `eak-core` is the only design truth; the only writer is `Engine::execute` (typed `Command`, expected revision, approval class, journal). UI, CLI, MCP, agents and retrieval never write project files. Knowledge and supplier caches live beside the project and are rebuildable, never design data. | ADR-0001..0004 | exists |
| Multi-domain model | Domain profiles (power, RF, analog, digital/high-speed, embedded) are typed data owned by the multi-domain worker, attached to requirements/constraints in the IR through new commands; a product may carry several. Domain rules are deterministic Rust in `eak-engineering`/`eak-validation`; DESK procedures stay advisory. | requirements model, ADR-0011 | classifier exists; IR profile not started |
| Requirements and constraints | Structured requirements (quantity, bound, condition, source) replace `Requirement{id,text}` via migration + commands; validation links criteria to requirements. Needs its own ADR when the IR changes. | `eak-core::migrate` | not started |
| Component identity | Engineering identity (`ComponentDef`: category, specs, operating conditions, abs-max, package, datasheet ref, symbol/footprint/pin map, 3D ref, lifecycle, provenance) is separate from supplier listings (LCSC/JLCPCB ids, stock, price), which are provider records linked by MPN/manufacturer and never proof of suitability. Datasheet values carry kind (recommended / abs-max), min/typ/max, conditions, package/revision scope and `extracted_auto` vs `human_verified`. Changing `ComponentDef` is a shared-contract change made with a migration. | `eak-core::library`, `migrate` | `SupplierOffer` + `supplier_offers` added (ADR-0028); datasheet-value types in `eak-core::datasheet` |
| Retrieval | Local-first: pure-Rust BM25 over provenance-carrying chunks (`eak-intelligence::knowledge`), exact MPN / LCSC-id lookup and metadata filters, engineering-aware rerank, optional dense backend behind a hook. No LlamaIndex/Qdrant/Haystack/RAGFlow (Python or service-based; see `docs/engineering/KNOWLEDGE_RETRIEVAL.md`). Output is evidence only: it cannot call tools or mutate state; every fact is labelled source-extracted / model interpretation / calculated / assumption / human-approved / unverified. | `sha2` | BM25, chunk provenance, advisory flag, 3 harness tools exist; rest not started |
| 3D pipeline | Extend the existing three.js viewer: model reference + source/provenance on the component, a Rust-side loader/cache converting to a simple mesh, per-footprint transform from placement, STEP/IGES conversion offline-first and licence-checked. Bodies without a model are listed as unknown height, not invented. | component identity, `scene3d.rs` | parametric bodies only |
| Validation | `eak-validation` stays the only verifier: revision- and input-hash-bound reports with `not_implemented`/`limitations`, fail-closed gates. Retrieved knowledge, AI output and simulation never count as a pass. | ADR-0021 | exists |
| Agent integration | Agents use the harness (typed tools, same `Command` path, standing approval by class, evidence labels, TTY-only human approval). Retrieval tools are category `knowledge`, never a design edit. | ADR-0018, 0020 | exists |
| UI | Engineering-first workspaces; no always-open assistant sidebar; knowledge shown as cited, labelled evidence (source, page, revision); keyboard-first command palette. | `docs/design-system.md` | partly exists |
| Resources | No resident service. Retrieval scores in memory per query (thousands of chunks); supplier calls off the UI thread with timeout, rate limit, cache, backoff and offline fallback so an outage never blocks the desktop. | provider trait | knowledge: yes; supplier provider: wired into desktop `parts_*` (ADR-0028, code-reviewed only) |

## Options considered
* External RAG platform or vector DB: rejected (Python, resident services, embeddings).
* SQLite FTS5 via bundled `rusqlite`: viable upgrade; rejected now (second storage authority, migration).
* Knowledge inside the project IR: rejected (not design truth; would churn revisions).

## Consequences
* New harness category `knowledge` (serialised `"knowledge"`) and tools `knowledge_search`, `knowledge_cite`,
  `knowledge_import`: an additive change to the tool envelope (`list_tools` gains a category).
* `eak-intelligence` gains a `sha2` dependency; `Cargo.lock` edited by hand.
* `<project>/knowledge/index.json` is a new rebuildable project file, outside `project.eak.json`.
* Component-identity, datasheet-value and supplier-provider types land as separate, migration-bearing
  commits; until then the table says "not started".

## Source requirement
Directive part 2 §7, §8, §11, Step 4; directive §2.6.
