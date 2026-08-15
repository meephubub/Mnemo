# Mnemo Roadmap

Tracks progress against the phased plan in `plan.md` (section
"Implementation Phases", ~line 1950 onward). See `build.md` for how
to build/run what exists so far.

## Done

- **Phase 0 — Architecture.** Cargo workspace with `mnemo-core`
  (data model/IDs/errors, no I/O), `mnemo-storage` (SQLite + FTS5),
  `mnemo-ingest` (pure parsing/chunking), `mnemo-search` (retrieval),
  and the top-level `mnemo` facade crate plus `mnemo-cli`.
- **Phase 1 — Basic Storage.** SQLite schema for sources, documents,
  chunks, conversations, messages, profile entries, and memories
  (`crates/mnemo-storage/src/migrations.rs`), applied idempotently on
  `Db::open`/`Db::open_in_memory`. Repository modules provide typed
  CRUD over every table.
- **Phase 2 — Document Ingestion (partial).** Plain text, Markdown
  (heading-aware sectioning), and a dependency-free HTML-to-text pass.
  Paragraph-aware chunking with a target/min character budget.
  Content-hash based dedup so re-ingesting an unchanged file/text is a
  no-op (`IngestOutcome::Unchanged`). **Not done:** PDF, DOCX, and
  email parsing (plan.md still calls these out under Phase 2).
- **Phase 3 — Full-Text Search.** FTS5 virtual tables + triggers for
  `chunks` and `messages`, BM25 ranking, lexical search across both
  scopes via `mnemo_search::search` / `Mnemo::search()`.
- **Phase 4 — Embeddings.** `mnemo-embeddings` crate defines an
  `Embedder` trait and a default `HashingEmbedder` (deterministic,
  dependency-free, L2-normalized vectors via the hashing trick).
  `Embedding` model + `embeddings` table/repository in storage.
  `Mnemo::embed()` handle with `embed_pending()` (incremental — only
  chunks lacking an embedding for the current model version are
  processed), `count()`, `count_pending()`, `clear()`, `get()`.
  Schema version bumped to 2. **Not done:** real ONNX/Candle model
  integration (the trait is ready for it), embedding cache, ANN index
  (currently brute-force cosine over all stored vectors).
- **Phase 5 — Hybrid Retrieval.** `RetrievalMode` enum
  (`Lexical`/`Vector`/`Hybrid`) on `SearchOptions`. `vector_search()`
  does brute-force cosine similarity over stored embeddings.
  `hybrid_search()` fuses lexical + vector signals via min-max
  normalized weighted score fusion (plan.md section 8). Default mode
  is `Hybrid`. Configurable `lexical_weight`/`vector_weight`.
- **Phase 7 — Context Packing.** `Mnemo::context()` handle with
  `pack()` / `pack_with_options()`. `pack_context()` searches,
  deduplicates near-identical chunks (same 100-char prefix), greedily
  packs into a token budget (~4 chars/token heuristic), respects
  `max_sources`, and returns `PackedContext` with `ContextChunk`s
  carrying citation metadata. `ContextRequest` struct with
  `query`/`token_budget`/`max_sources`.

## Not started

Everything below is unimplemented; the facade's public API
(`SearchHit.score`, `SearchScope`, etc.) was designed so these can be
added without breaking existing callers.

- **Phase 6 — Reranking.** No two-stage reranker; `mnemo_search`
  returns fused results directly.
- **Phase 8 — Conversation Memory.** Conversations/messages are
  stored and searchable, but there's no summarization or
  memory-extraction pipeline over them yet.
- **Phase 9 — User Profile (partial).** Storage + CRUD exist
  (`Mnemo::profile()`), but there's no automatic extraction/update
  pipeline applying the confidence-threshold rules from plan.md
  section 22 — `ProfileHandle::set` just takes a confidence you pass
  in.
- **Phase 10 — Memory Lifecycle (partial).** The `Memory` model,
  status enum, and `MemoryStore` CRUD (`add`/`list`/`update`/
  `set_status`/`supersede`/`delete`) exist, but nothing automatically
  transitions memories between states (candidate → active → expired,
  etc.) or runs the promotion/decay policy from plan.md sections 24-26.
- **Phase 11 — Temporal Memory.** `Memory.valid_from`/`valid_until`
  fields exist in the schema/model but nothing sets or queries them.
- **Phase 12 — Entity Extraction.**
- **Phase 13 — Knowledge Graph.** `EntityId`/`RelationshipId`/
  `EventId` are reserved in `mnemo-core::ids` but have no models,
  tables, or repositories yet.
- **Phase 14 — Contradictions.** `Memory.superseded_by` and
  `memories::supersede` exist as low-level plumbing, but there's no
  detection logic that decides two memories conflict.
- **Phase 15 — Background Processing.** Everything runs synchronously
  inside the calling request (via `spawn_blocking`); there's no job
  queue, scheduler, or `IngestionJobId` usage yet (the ID type is
  reserved in `mnemo-core::ids`).
- **Phase 16 — Provenance and Citations (partial).** `SearchHit`
  already carries `document_title`/`source_name`/`section`/`page` for
  citation rendering, and `Source`/`Document` track provenance, but
  there's no dedicated citation-formatting API.
- **Phase 17 — Agent Integration.**
- **Phase 18 — API/MCP.** Only a library (`mnemo`) and CLI
  (`mnemo-cli`) exist; no HTTP/MCP server.
- **Phase 19 — Security.** No encryption at rest, access control, or
  redaction beyond the `Sensitivity` enum already on `Source`.
- **Phase 20 — Evaluation.**
- **Phase 21 — Optimization.**
- **Phase 22 — Multimodal Knowledge.**
- **Phase 23 — Connectors.**

## Suggested next steps

1. Add `cargo test` coverage for `mnemo-ingest` chunking/parsing and
   `mnemo-storage` repositories (currently untested).
2. Phase 6: implement a reranker abstraction (`trait Reranker`) and
   wire it into the hybrid search pipeline as an optional second stage
   (plan.md section 10).
3. Phase 2: add PDF/DOCX parsers behind the existing `FileKind`
   enum in `mnemo-ingest`.
4. Phase 10: implement the promotion/decay policy described in
   plan.md sections 24-26 as a function callers can run periodically
   over `MemoryStore::list(Some(MemoryStatus::Candidate))`.
5. Phase 4: replace `HashingEmbedder` with a real local embedding
   model (ONNX/Candle) behind the existing `Embedder` trait, and add
   an ANN index for sub-linear vector search at scale.
