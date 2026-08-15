# Mnemo Roadmap

Tracks progress against the phased plan in `plan.md` (section
"76. Development Phases" onward, ~line 1950). See `build.md` for the
exact `cargo` commands to build/check/test what exists so far (none
of them have been run in this environment — see the note at the top
of `build.md`).

This file intentionally only describes what is actually wired up in
`crates/` and `src/` — accessible from the public `mnemo` facade
crate and exercised by a test where one exists — not what a crate's
doc-comments aspire to. The `mnemo-cli` crate is excluded from scope
here per project direction; it is not being extended alongside the
library.

## Done

- **Phase 0 — Architecture.** Cargo workspace with `mnemo-core`
  (data model/IDs/errors, no I/O), `mnemo-storage` (SQLite + FTS5),
  `mnemo-ingest` (pure parsing/chunking), `mnemo-embeddings`
  (embedding trait + hashing embedder), `mnemo-search` (retrieval),
  and the top-level `mnemo` facade crate plus `mnemo-cli` (out of
  scope for this roadmap).
- **Phase 1 — Basic Storage.** SQLite schema for sources, documents,
  chunks, conversations, messages, profile entries, memories, and
  (as of this pass) embeddings
  (`crates/mnemo-storage/src/migrations.rs`), applied idempotently on
  `Db::open`/`Db::open_in_memory`. Repository modules provide typed
  CRUD over every table, including `repositories::embeddings`.
- **Phase 2 — Document Ingestion (partial).** Plain text, Markdown
  (heading-aware sectioning), and a dependency-free HTML-to-text pass.
  Paragraph-aware chunking with a target/min character budget.
  Content-hash based dedup so re-ingesting an unchanged file/text is a
  no-op (`IngestOutcome::Unchanged`). **Not done:** PDF, DOCX, and
  email parsing (plan.md still calls these out under Phase 2).
- **Phase 3 — Full-Text Search.** FTS5 virtual tables + triggers for
  `chunks` and `messages`, BM25 ranking, lexical search across both
  scopes via `mnemo_search::search` / `Mnemo::search().search()`.
- **Phase 4 — Embeddings.** Real, end-to-end wiring (previously the
  `mnemo-embeddings` crate existed but was **not** a workspace member
  and was not reachable from the facade — that has been fixed):
  - `mnemo-embeddings::Embedder` trait + `HashingEmbedder` (deterministic,
    dependency-free, L2-normalized vectors via the hashing trick).
  - `embeddings` table (`chunk_id`, `model_name`, `model_version`,
    `dimension`, `vector` as JSON, unique on
    `(chunk_id, model_name, model_version)`) and
    `mnemo_storage::repositories::embeddings` (`upsert`, `get`,
    `list_by_model`, `list_pending_chunk_ids`, `count`,
    `count_pending`, `clear`). Schema version bumped 1 → 2.
  - `Mnemo::embed()` / `Mnemo::embed_with(embedder)` → `EmbedHandle`
    with `embed_pending()` (incremental — only chunks lacking an
    embedding for the handle's model/version are processed),
    `count()`, `count_pending()`, `clear()`, `get(chunk_id)`,
    `embedder()`.
  - **Not done:** real ONNX/Candle model integration (the trait is
    ready for it), an embedding cache, an ANN index (vector search is
    brute-force cosine over all stored vectors for a model).
- **Phase 5 — Hybrid Retrieval (initial).** `mnemo_search::vector_search`
  (brute-force cosine over the `embeddings` table for a given
  embedder) and `mnemo_search::hybrid_search` (min-max normalized,
  weighted fusion of lexical + vector candidate pools via
  `HybridWeights { lexical_weight, vector_weight }`), exposed on the
  facade as `SearchHandle::search_vector` / `search_hybrid`. Unit
  tests in `crates/mnemo-search/src/lib.rs` cover lexical, vector, and
  hybrid search against an in-memory db. **Not done:** entity/recency/
  importance signals in the fusion score (plan.md section 8 lists
  these as later additions), embedding of conversation messages (only
  chunks are embedded today, so hybrid fusion only affects document
  hits), retrieval benchmarks.
- **Phase 7 — Context Packing.** `mnemo_search::context::pack_context`
  runs `hybrid_search` for a candidate pool, drops near-duplicate
  chunks (word-set Jaccard similarity ≥ 0.85), then greedily packs the
  rest into a `token_budget` (≈4 chars/token estimate) while capping
  distinct sources at `max_sources` — exposed on the facade as
  `Mnemo::context()` / `Mnemo::context_with(embedder)` →
  `ContextHandle::pack` / `pack_with_request`. `PackedContext` carries
  `chunks: Vec<ContextChunk>` (each with its `SearchHit` and estimated
  token count), `estimated_tokens`, and the fully-hydrated `sources:
  Vec<Source>` used, for citation rendering. `SearchHit` gained a
  `source_id` field (previously only `source_name: Option<String>`)
  so packing/diversity selection and source hydration have a stable
  key instead of matching on a display name. Unit tests in
  `crates/mnemo-search/src/context.rs` cover token-budget
  enforcement, source-diversity capping, and near-duplicate dropping.
  **Not done:** "preserve surrounding context where needed" (plan.md's
  neighbor-chunk expansion — e.g. pulling in `chunk_index - 1`/`+ 1`
  for a selected chunk), true optimal packing (this is greedy-by-score,
  not a knapsack solve — see the doc comment on `pack_context` for why
  that's an intentional tradeoff), reranking before packing (Phase 6).

## Not started

- **Phase 6 — Reranking.** No `Reranker` trait or two-stage pipeline;
  `hybrid_search` (and therefore `pack_context`, which builds on it)
  returns fused results directly as the final ranking.
- **Phase 8 — Conversation Memory.** Conversations/messages are
  stored and searchable, but there's no summarization or
  memory-extraction pipeline over them, and messages aren't embedded
  (so vector/hybrid search only covers document chunks today).
- **Phase 9 — User Profile (partial).** Storage + CRUD exist
  (`Mnemo::profile()`), but there's no automatic extraction/update
  pipeline applying the confidence-threshold rules from plan.md
  section 22 — `ProfileHandle::set` just takes a confidence you pass
  in.
- **Phase 10 — Memory Lifecycle (partial).** The `Memory` model,
  status enum, and `MemoryStore` CRUD (`add`/`list`/`update`/
  `set_status`/`supersede`/`delete`/`propose`/`promote_ready`/
  `expire_temporary`) exist, but nothing automatically runs this on a
  schedule — callers must invoke `promote_ready`/`expire_temporary`
  themselves.
- **Phase 11 — Temporal Memory.** `Memory.valid_from`/`valid_until`
  fields exist in the schema/model and have setters
  (`MemoryStore::set_valid_range`), but nothing queries "what was true
  at time T" or resolves temporal contradictions yet.
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
- **Phase 18 — API/MCP.** Explicitly out of scope per plan.md section
  9 (G9): the facade is a Rust library only, no server/MCP layer.
- **Phase 19 — Security.** No encryption at rest, access control, or
  redaction beyond the `Sensitivity` enum already on `Source`.
- **Phase 20 — Evaluation.**
- **Phase 21 — Optimization.**
- **Phase 22 — Multimodal Knowledge.**
- **Phase 23 — Connectors.**

## Suggested next steps

1. Phase 6: implement a `Reranker` trait and wire it into
   `pack_context` (and/or `hybrid_search`) as an optional second stage
   over the fused candidate pool, before packing (plan.md section 10
   / section 82). (already partially implemented see rerank.rs in the mnemo-search crate)
2. Phase 7 follow-up: neighbor-chunk expansion ("preserve surrounding
   context where needed") — when a chunk is selected, optionally pull
   in `chunk_index - 1` / `chunk_index + 1` from the same document if
   the token budget allows, so packed context isn't mid-sentence at
   its edges.
3. Add `cargo test` coverage for `mnemo-storage` repositories and
   `mnemo-ingest` chunking/parsing (still untested; only
   `mnemo-embeddings` and `mnemo-search` have unit tests).
4. Phase 2: add PDF/DOCX parsers behind the existing `FileKind`
   enum in `mnemo-ingest`.
5. Phase 8: embed conversation messages the same way chunks are
   embedded today, so `vector_search`/`hybrid_search`/`pack_context`
   can cover conversation history, not just documents.
6. Phase 10: implement the promotion/decay policy described in
   plan.md sections 24-26 as a function callers can run periodically
   (building on the existing `MemoryStore::promote_ready`/
   `expire_temporary`).
7. Phase 4 follow-up: replace `HashingEmbedder` with a real local
   embedding model (ONNX/Candle) behind the existing `Embedder` trait,
   and add an ANN index for sub-linear vector search at scale.
