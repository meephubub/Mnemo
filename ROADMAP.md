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
  (heading-aware sectioning), a hand-rolled HTML-to-text pass,
  PDF (`pdf-extract`, one section per page with an accurate 1-based
  `Chunk::page`), and DOCX (`.docx` is a ZIP container; a hand-rolled
  scan of `word/document.xml` sections on Word's
  built-in heading styles and pulls the title from `docProps/core.xml`
  or a `Title`-styled paragraph). `parsers::parse` now takes raw bytes
  rather than a UTF-8 string so binary formats share the same pipeline
  as text ones; `ingest_path`/`ingest_bytes_with_config` read/accept
  bytes accordingly, and `Chunk::page` is threaded through end-to-end
  from parsing to persistence. Paragraph-aware chunking with a
  target/min character budget. Content-hash based dedup so
  re-ingesting an unchanged file/text is a no-op
  (`IngestOutcome::Unchanged`). **Not done:** email parsing (plan.md
  still calls this out under Phase 2).
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
  hybrid search against an in-memory db. As of this pass, conversation
  messages are embedded and retrievable the same way document chunks
  are (previously only chunks were embedded, so vector/hybrid fusion
  only ever affected document hits — see the `Not done` note this
  replaces): a new `message_embeddings` table + `MessageEmbedding`
  model (mirrors `embeddings`/`Embedding` exactly, keyed on
  `message_id` instead of `chunk_id` — schema version bumped 2 → 3),
  `mnemo_storage::repositories::message_embeddings` (`upsert`, `get`,
  `list_by_model`, `list_pending_message_ids`, `count`,
  `count_pending`, `clear`), and `EmbedHandle::embed_pending_messages`
  / `count_messages` / `count_pending_messages` / `clear_messages` /
  `get_message` alongside the existing chunk-only methods.
  `vector_search` now scans both `embeddings` and `message_embeddings`
  and merges the two candidate pools by cosine score before
  truncating to `limit`; `hybrid_search` fuses lexical and vector hits
  by a `(kind, id)` identity (previously `ChunkId`-only, so a
  `hybrid_search` call for `SearchScope::Conversations` fused lexical
  message hits with *no* vector signal at all) and now filters
  `vector_search`'s output by `SearchOptions::scope` before fusing
  (`vector_search` itself has no scope parameter — every call scans
  every stored vector for the model, same as before). New tests in
  `crates/mnemo-search/src/lib.rs`
  (`vector_search_ranks_semantically_closer_message_first`,
  `hybrid_search_fuses_message_vector_hit_with_lexical_hit`,
  `hybrid_search_documents_scope_excludes_message_vector_hits`) cover
  message vector ranking, hybrid fusion including a message hit, and
  scope filtering excluding message vector hits from a
  `Documents`-scoped hybrid search. **Not done:** entity/recency/
  importance signals in the fusion score (plan.md section 8 lists
  these as later additions), retrieval benchmarks, an automatic
  "embed new messages as they're added" hook (`embed_pending_messages`
  must still be called explicitly, same as `embed_pending` for
  chunks).
- **Phase 6 — Reranking.** `mnemo_search::rerank` module (previously
  written but not wired into anything — not declared in
  `mnemo-search`'s `lib.rs`, so it was unreachable from outside the
  crate) is now a real module: a `Reranker` trait (`score(query, hits)
  -> Vec<f64>`, one score per hit) and a `rerank()` function that
  re-scores and re-sorts a candidate pool, plus `HeuristicReranker` —
  a dependency-free default that blends the incoming Stage 1 score
  with query/body exact-phrase token overlap and a title/section
  match boost. Wired into `pack_context` as a genuinely optional Stage
  2: `ContextRequest` gained a `reranker: Option<Arc<dyn Reranker>>`
  field (`None` by default, so existing callers are unaffected) and a
  `with_reranker()` builder method; when set, `pack_context` runs it
  over the fused `hybrid_search` candidate pool *before*
  dedup/packing, so the reranker's scores (not just Stage 1's fused
  scores) decide what survives near-duplicate dropping and greedy
  packing. Exposed on the facade as `Reranker`/`HeuristicReranker`/
  `rerank` re-exports plus `ContextHandle::pack_with_reranker`. Unit
  tests in `crates/mnemo-search/src/rerank.rs` (reordering, title-match
  boosting, empty-input no-op, mismatched-score-count error) and two
  new tests in `crates/mnemo-search/src/context.rs`
  (`pack_context_applies_configured_reranker`,
  `pack_context_without_reranker_skips_stage_two`) covering both the
  `Some`/`None` branches through `pack_context` itself. **Not
  wired:** `hybrid_search` itself still returns fused results
  directly — reranking is only available as `pack_context`'s Stage 2,
  matching plan.md's "the reranker should be optional" framing; a
  caller who wants ranked hits (not a packed context) with reranking
  applied would need to call `mnemo_search::rerank::rerank` directly
  against a `hybrid_search` pool themselves. Still dependency-free
  (`HeuristicReranker`) — no learned cross-encoder model integration
  (BGE/Jina/ONNX), which the `Reranker` trait leaves room for but does
  not implement.
- **Phase 7 — Context Packing.** `mnemo_search::context::pack_context`
  runs `hybrid_search` for a candidate pool, optionally reranks it
  (Phase 6, see above), drops near-duplicate chunks (word-set Jaccard
  similarity ≥ 0.85), then greedily packs the rest into a
  `token_budget` (≈4 chars/token estimate) while capping distinct
  sources at `max_sources`, and (as of this pass) optionally expands
  each selected chunk with its immediate document neighbors — exposed
  on the facade as `Mnemo::context()` / `Mnemo::context_with(embedder)`
  → `ContextHandle::pack` / `pack_with_request` / `pack_with_reranker`
  / `pack_with_neighbor_expansion`. `PackedContext` carries `chunks:
  Vec<ContextChunk>` (each with its `SearchHit` and estimated token
  count), `estimated_tokens`, and the fully-hydrated `sources:
  Vec<Source>` used, for citation rendering. `SearchHit` gained a
  `source_id` field (previously only `source_name: Option<String>`)
  so packing/diversity selection and source hydration have a stable
  key instead of matching on a display name. Unit tests in
  `crates/mnemo-search/src/context.rs` cover token-budget enforcement,
  source-diversity capping, near-duplicate dropping, reranker wiring,
  and (as of this pass) neighbor expansion.
  - **"Preserve surrounding context where needed" (neighbor-chunk
    expansion)** is now implemented as an opt-in post-pack step:
    `ContextRequest` gained a `neighbor_expansion: bool` field
    (`false` by default) and a `with_neighbor_expansion()` builder;
    when set, `pack_context` runs a new `expand_with_neighbors` pass
    after the main greedy pack (deliberately *after*, not folded in —
    see that function's doc comment for why) that looks up each
    selected document chunk's `chunk_index - 1` / `+ 1` siblings via
    a new `mnemo_storage::repositories::chunks::get_by_document_and_index`
    and pulls in whichever ones still fit the remaining
    `token_budget`. Neighbors inherit their parent's provenance
    (`document_title`/`source_id`/`source_name`/`score`) but keep
    their own `section`/`page`. Three new tests
    (`pack_context_expands_neighbor_chunks_when_enabled`,
    `pack_context_neighbor_expansion_respects_token_budget`,
    `pack_context_neighbor_expansion_disabled_by_default`) cover
    enabling it, its budget interaction, and the disabled-by-default
    regression case, using a new `seed_single_document` test helper
    (chunks sharing one document, unlike `seed_multi_source`'s
    one-document-per-chunk).
  - **Not done:** true optimal packing (this is greedy-by-score, not
    a knapsack solve — see the doc comment on `pack_context` for why
    that's an intentional tradeoff).

## Not started

- **Phase 8 — Conversation Memory (partial).** Conversations/messages
  are stored, searchable, and (as of this pass) embedded/retrievable
  via vector and hybrid search alongside document chunks — see Phase
  5 above. **Not done:** there's still no summarization or
  memory-extraction pipeline over conversation history (turning
  messages into `Memory`/`ProfileEntry` records), which is what this
  phase is really about; embedding coverage was only ever the
  retrieval-side prerequisite for it.
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
- **Phase 22 — Multimodal Knowledge.** Includes a **CLAP/CLIP-style
  joint embedding search** sub-feature: ingest images and audio
  clips as first-class sources and make them searchable by a text
  query (or by each other) via a shared embedding space, not just by
  filename/metadata or a transcript. Sketch of how this slots into
  the existing architecture rather than bolting on a parallel system:
  - `mnemo-core`: new `Source`/`Document` kind(s) for image and audio
    media (or a `MediaKind` alongside the existing `Sensitivity`/
    provenance fields on `Source`) so a photo or clip is a `Source`
    like any ingested file, plus enough metadata (duration, sample
    rate, dimensions, codec) to render a citation back to it.
  - `mnemo-ingest`: image/audio don't chunk into paragraphs the way
    text does, so ingestion here means decoding the file
    (`image`/`symphonia` are the natural pure-Rust choices, matching
    this project's no-external-tool constraint used for
    `pdf-extract`/`zip`) and producing the fixed-size input a
    CLAP/CLIP model expects (a resampled waveform, or a resized
    frame) rather than a `ParsedSection`/`ChunkDraft` — an audio clip
    would still likely need to be windowed into fixed-length segments
    for embedding, which *is* analogous to chunking.
  - `mnemo-embeddings`: the current `Embedder` trait is `&str -> Vec<f32>`-only,
    so it can't take an image or audio buffer as input, and a
    CLAP/CLIP model has a *joint* text/audio (or text/image) space —
    meaning the same model class must expose both a text-embedding
    path and a media-embedding path into that shared space. This
    needs a new trait (e.g. `MultimodalEmbedder`, with `embed_text`
    and `embed_audio`/`embed_image` methods returning vectors
    comparable by cosine similarity to each other) rather than
    reusing `Embedder` as-is, so a text query embeds into the same
    space a stored clip's embedding lives in.
  - `mnemo-storage`: a new table mirroring `embeddings`/
    `message_embeddings`'s shape (keyed on a media/document id, model
    name/version, dimension, vector) so it drops into the same
    dedup/model-versioning pattern already established, rather than a
    bespoke schema.
  - `mnemo-search`: `vector_search` already merges multiple embedding
    tables into one candidate pool by cosine score (it does this
    today for `embeddings` + `message_embeddings`) — a media
    embedding table is a third pool to merge the same way, so a text
    query's `embed_text` vector can rank a photo or audio clip
    alongside document chunks and messages in one hybrid search
    without a separate "media search" API.
  - Practical note: unlike the rest of this project's dependency-free/
    pure-Rust posture (`HashingEmbedder`, `pdf-extract`, `zip`), a
    real CLAP/CLIP model is a large pretrained neural net — running
    one locally means an ONNX Runtime or Candle inference path (the
    `Embedder` trait was already written with this in mind for text
    models; the same reasoning applies to whatever multimodal trait
    replaces/extends it here), which is a meaningfully bigger lift
    than the hashing-trick default this project currently ships.
- **Phase 23 — Connectors.**

## Suggested next steps

1. Add `cargo test` coverage for the rest of `mnemo-storage`
   repositories (still untested; `mnemo-embeddings`, `mnemo-search`,
   and — as of this pass — `mnemo-ingest` all have coverage now, plus
   `chunks::get_by_document_and_index` via `mnemo-search`'s
   neighbor-expansion tests).
2. ~~Phase 2: add PDF/DOCX parsers behind the existing `FileKind`
   enum in `mnemo-ingest`.~~ Done this pass — see Phase 2 above.
3. Phase 8: build a summarization/memory-extraction pipeline over
   conversation history (turning messages into `Memory`/
   `ProfileEntry` records via the confidence-threshold rules in
   plan.md section 22) — the retrieval-side prerequisite (embedding
   messages so `vector_search`/`hybrid_search` cover conversation
   history, not just documents) is now done; `pack_context` still
   only pulls from `hybrid_search`'s candidate pool, so it already
   picks up message hits for free once a caller requests
   `SearchScope::All`/`Conversations`.
4. Phase 10: implement the promotion/decay policy described in
   plan.md sections 24-26 as a function callers can run periodically
   (building on the existing `MemoryStore::promote_ready`/
   `expire_temporary`).
5. Phase 4 follow-up: replace `HashingEmbedder` with a real local
   embedding model (ONNX/Candle) behind the existing `Embedder` trait,
   and add an ANN index for sub-linear vector search at scale.
6. Phase 6 follow-up: a learned cross-encoder `Reranker` implementation
   (BGE/Jina/other ONNX model) as an alternative to `HeuristicReranker`,
   and/or exposing reranking directly on `hybrid_search`'s result (not
   just `pack_context`'s Stage 2) for callers that want ranked hits
   without packing them into a context.
7. Phase 7 follow-up: `expand_with_neighbors` only pulls in the
   immediate `± 1` sibling today; a `neighbor_radius` option (or
   similar) could let callers ask for more than one chunk of
   surrounding context on either side, still gated by the token
   budget.
8. Phase 22: CLAP/CLIP-style joint audio/image/text search (see Phase
   22 above for the full sketch) — new `MultimodalEmbedder` trait in
   `mnemo-embeddings`, image/audio decoding in `mnemo-ingest`, a media
   embeddings table in `mnemo-storage` mirroring `embeddings`'s shape,
   and a third pool for `vector_search`/`hybrid_search` to merge
   alongside `embeddings`/`message_embeddings`. The biggest departure
   from every other phase done so far: it requires an actual
   pretrained model (ONNX/Candle inference), not a dependency-free
   default like `HashingEmbedder`.
