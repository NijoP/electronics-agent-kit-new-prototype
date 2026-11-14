# Engineering knowledge retrieval

Directive §2.6 / §8. Status: **code-reviewed only** (never compiled or run; tests written, unrun).
Decision record: `docs/decisions/0024-canonical-state-and-knowledge-architecture.md`.

## Candidates (public information, not re-verified in this session; nothing was installed or run)

| Project | Architecture | Resource cost for a desktop app | Licence | Verdict |
|---|---|---|---|---|
| LlamaIndex | Python (and TS) orchestration framework: loaders, node parsers, indices, query engines over a vector store | Python runtime plus an embedding model or API; no storage of its own | MIT | **Reject.** Python is banned in EAK; its useful ideas (chunking with metadata, hybrid retrieval, citations) are small. |
| Qdrant | Rust vector database; separate server process, HNSW over dense vectors, payload filters | Needs an embedding model to be useful; a resident service and RAM proportional to vectors | Apache-2.0 | **Reject as a default** (always-running service, embeddings). Keep as an optional dense backend behind the hybrid hook. |
| Haystack | Python pipelines of retrievers, rankers, readers | Python runtime plus document store and models | Apache-2.0 | **Reject** (Python). Its retriever + ranker split is the pattern adopted here. |
| RAGFlow | Deep-document-understanding RAG platform: web service with a doc store, object store, DB and cache, run with containers | Multi-container stack, GBs of RAM | Apache-2.0 | **Reject** for a desktop app. Its layout-aware table extraction is the idea worth borrowing, as a future importer. |

Licences are noted for awareness only; no code from any of them is used.

## Choice: pure-Rust BM25 over chunks, no embeddings

`eak-intelligence::knowledge` (`crates/eak-intelligence/src/knowledge.rs`):

* **Index**: a list of `Chunk`s, scored with BM25 (k1 1.2, b 0.75) at query time. No inverted index on
  disk, no service, no new dependency beyond `sha2`. Fine for thousands of chunks; a persisted
  inverted index or SQLite FTS5 (`rusqlite` is already bundled in `eak-runtime`) is the upgrade path
  if corpora outgrow this. FTS5 was not chosen now because it adds a second storage authority and
  schema migration for no benefit at this size, and BM25 here is fully deterministic and testable.
* **Corpus**: built-in DESK-derived rules (`mental.rs`: M1–M6, the 12-step procedure, derating
  facts), `docs/engineering/{ELECTRICAL_RULES,IPC_RULE_VALUES}.md` embedded at build time, and text the
  user imports (`knowledge_import`) into `<project>/knowledge/index.json`.
* **Provenance per chunk**: source name, locator (`page N, section H` / `section H` / `pN`),
  revision label (project revision at import, or build/DESK version), SHA-256 of the whole source and of
  the chunk, deterministic chunk id.
* **Ingestion**: UTF-8 text only; form feed = page break, markdown headings = sections. PDFs are **not**
  parsed (limitation): extract text outside EAK and import it. Re-importing a source name replaces its
  chunks. Import is confined to files inside the project folder, max 5 MiB.
* **Trust boundary**: results carry `advisory: "advisory_knowledge_not_a_validation_result"` and a note.
  The tools never call `Engine::execute`, never write evidence, never feed `eak-validation` gates.
  Retrieved text is data, not instructions.

## Retrieval features (part 2 §8.3)

* **Exact lookup**: `by_part` / `knowledge_lookup` by MPN (given as `mpn` at import) or LCSC id (found in text).
* **Metadata filter**: `kind`, `part` on `knowledge_search`.
* **Engineering-aware rerank** (`rerank_factor`, deterministic): x2 for an exact part match, x1.25 when the
  query asks for "absolute maximum" / "recommended" / "typical" and the chunk states that class.
* **Hybrid hook**: `DenseScorer` + `search_hybrid`; EAK ships no embedding model, so dense retrieval is off.
* **Labels and citations**: each chunk has a `FactLabel` (`eak-core::datasheet`); citations carry source,
  locator (page/section), revision, hashes, label and part ids. Datasheet values with class
  (absolute max / recommended / characteristic), min/typ/max, conditions, package/revision scope and
  extracted-auto vs human-verified are typed in `eak-core::datasheet::DatasheetValue`; nothing extracts
  them automatically yet.
* **Incremental ingestion**: re-importing a source replaces only its chunks.

## Harness tools (category `knowledge`, in `tool_specs()`)

`knowledge_search {query, limit?, kind?, part?}` · `knowledge_lookup {part}` · `knowledge_cite {chunk_id}` ·
`knowledge_import {path, source?, mpn?}`. Implemented in `crates/eak-harness/src/knowledge.rs`.

## Not implemented (do not report as available)

* Table / parameter extraction from datasheets (min/typ/max, conditions), PDF parsing, OCR.
* The dense scorer itself (only the hook exists).
* Labels other than chunk-level `source_extracted` / `unverified`: model-interpretation, calculated,
  assumption and human-approved are defined but nothing assigns them yet.
* LCSC-id detection also matches footprint-like tokens such as `C0805` (affects exact lookup only).
* File-change detection (re-import is explicit); synonyms (lexical match only: an empty result is not
  evidence that a topic is unaddressed).
