//! Context packing (plan.md section 11 "Context Packing" / section
//! 83 "Phase 7").
//!
//! Turns a ranked candidate pool from [`crate::hybrid_search`] into a
//! [`PackedContext`] that respects a token budget and a max-sources
//! cap, per plan.md section 11's requirements:
//!
//! - Rank candidates (delegated to `hybrid_search`).
//! - Remove duplicates / avoid redundant chunks (near-duplicate
//!   detection via word-set Jaccard similarity).
//! - Respect token budget (greedy knapsack-style packing, see
//!   [`pack_context`] for why greedy rather than optimal).
//! - Prefer diverse sources (`max_sources` caps distinct
//!   `Source`s; already-included sources are preferred once the cap
//!   is hit).
//! - Preserve citations (`ContextChunk` carries the full `SearchHit`,
//!   which already has document/source/section/page provenance).
//!
//! Reranking (plan.md section 10 / Phase 6) plugs in as an optional
//! second stage: when [`ContextRequest::reranker`] is set,
//! [`pack_context`] runs it over the fused `hybrid_search` candidate
//! pool — before dedup/packing, so the reranker's scores (not just
//! Stage 1's fused scores) drive which chunks survive near-duplicate
//! dropping and greedy packing.
//!
//! "Preserve surrounding context where needed" (plan.md's
//! neighbor-chunk expansion) is implemented as an opt-in post-pack
//! step: when [`ContextRequest::neighbor_expansion`] is set, for each
//! selected chunk [`pack_context`] looks up its immediate
//! `chunk_index - 1` / `chunk_index + 1` siblings in the same
//! document and pulls in whichever ones still fit the remaining
//! token budget, so a chunk that got cut off mid-sentence at either
//! edge has its neighbor available too. See [`expand_with_neighbors`]
//! for why this runs *after* the main greedy pack rather than being
//! folded into it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mnemo_core::ids::ChunkId;
use mnemo_core::models::Source;
use mnemo_storage::repositories::{chunks, sources};
use mnemo_storage::Db;
use mnemo_embeddings::Embedder;
use rusqlite::Connection;

use crate::error::Result;
use crate::rerank::Reranker;
use crate::{hybrid_search, HitKind, HybridWeights, SearchHit, SearchOptions, SearchScope};

/// Input to [`pack_context`] (plan.md section 11's `ContextRequest`).
#[derive(Clone)]
pub struct ContextRequest {
    pub query: String,
    pub token_budget: usize,
    pub max_sources: usize,
    /// Fusion weights forwarded to `hybrid_search` when gathering the
    /// candidate pool.
    pub weights: HybridWeights,
    /// Which part of the knowledge base to draw candidates from.
    pub scope: SearchScope,
    /// How many fused candidates to consider before packing. Should
    /// be comfortably larger than what the token budget will actually
    /// fit, so the packer has room to skip duplicates/over-budget
    /// chunks and still hit the budget. Defaults to `50` (see
    /// `Default`), matching plan.md section 10's "~50 candidates"
    /// Stage 1 output size.
    pub candidate_pool: usize,
    /// Optional Stage 2 reranker (plan.md section 10 / Phase 6),
    /// applied to the fused `hybrid_search` candidate pool before
    /// dedup/packing. `None` (the default) skips reranking entirely,
    /// so `pack_context` behaves exactly as it did before Phase 6 —
    /// per plan.md, "the reranker should be optional".
    pub reranker: Option<Arc<dyn Reranker>>,
    /// When `true`, after the main pack each selected document chunk
    /// gets its immediate previous/next chunk (by `chunk_index`) from
    /// the same document pulled in too, budget permitting — plan.md's
    /// "preserve surrounding context where needed". Defaults to
    /// `false`: an opt-in step, since it trades some of the token
    /// budget that would otherwise go to more independently-ranked
    /// chunks for continuity around the chunks already selected.
    pub neighbor_expansion: bool,
}

impl Default for ContextRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            token_budget: 2000,
            max_sources: 5,
            weights: HybridWeights::default(),
            scope: SearchScope::All,
            candidate_pool: 50,
            reranker: None,
            neighbor_expansion: false,
        }
    }
}

impl std::fmt::Debug for ContextRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextRequest")
            .field("query", &self.query)
            .field("token_budget", &self.token_budget)
            .field("max_sources", &self.max_sources)
            .field("weights", &self.weights)
            .field("scope", &self.scope)
            .field("candidate_pool", &self.candidate_pool)
            .field(
                "reranker",
                &self.reranker.as_ref().map(|r| r.name()),
            )
            .field("neighbor_expansion", &self.neighbor_expansion)
            .finish()
    }
}

impl ContextRequest {
    pub fn new(query: impl Into<String>, token_budget: usize) -> Self {
        Self {
            query: query.into(),
            token_budget,
            ..Default::default()
        }
    }

    /// Set a Stage 2 reranker to run over the candidate pool before
    /// dedup/packing (plan.md section 10 / Phase 6). Builder-style
    /// convenience over setting [`Self::reranker`] directly.
    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Enable neighbor-chunk expansion: after the main pack, pull in
    /// each selected chunk's immediate document siblings when they
    /// fit the remaining token budget. Builder-style convenience over
    /// setting [`Self::neighbor_expansion`] directly.
    pub fn with_neighbor_expansion(mut self, enabled: bool) -> Self {
        self.neighbor_expansion = enabled;
        self
    }
}

/// A single chunk selected into a [`PackedContext`], with its
/// estimated token cost alongside the original hit for citation
/// rendering.
#[derive(Debug, Clone)]
pub struct ContextChunk {
    pub hit: SearchHit,
    pub estimated_tokens: usize,
}

/// Output of [`pack_context`] (plan.md section 11's `Context`).
#[derive(Debug, Clone)]
pub struct PackedContext {
    pub chunks: Vec<ContextChunk>,
    pub estimated_tokens: usize,
    pub sources: Vec<Source>,
}

/// Cheap, dependency-free token estimate. Real tokenizers vary
/// (BPE, wordpiece, ...) and pulling one in is unnecessary for a
/// *budget*, where being approximately right and consistent matters
/// more than matching any specific model's tokenizer exactly. `~4`
/// characters per token is the standard rule-of-thumb approximation
/// for English text used by OpenAI/Anthropic-style docs.
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    (chars as f64 / 4.0).ceil() as usize
}

/// Normalize text for near-duplicate comparison: lowercase, drop
/// punctuation, collapse whitespace into a word set.
fn word_set(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Jaccard similarity between two word sets: `|A ∩ B| / |A ∪ B|`.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Above this similarity, two chunks are considered redundant and
/// only the higher-ranked one is kept.
const NEAR_DUPLICATE_THRESHOLD: f64 = 0.85;

/// Run hybrid retrieval, deduplicate near-identical chunks, and
/// greedily pack the remaining candidates into `request.token_budget`
/// while preferring to spread selections across up to
/// `request.max_sources` distinct sources.
///
/// Packing is greedy (highest fused score first), not an optimal
/// knapsack solve: plan.md's own worked example ("Select A + B + D"
/// over the tied-budget "A + B + C") is itself just "take by score
/// order until the next candidate doesn't fit, then keep scanning for
/// one that does" — which is exactly what a greedy-with-lookahead
/// pass does, and it's O(n) instead of exponential.
pub fn pack_context(db: &Db, embedder: &dyn Embedder, request: &ContextRequest) -> Result<PackedContext> {
    let search_options = SearchOptions {
        scope: request.scope,
        limit: request.candidate_pool,
    };
    let candidates = hybrid_search(db, embedder, &request.query, &search_options, request.weights)?;

    // Stage 2 (plan.md section 10 / Phase 6): optionally rerank the
    // fused candidate pool before dedup/packing, so a reranker's
    // scores — not just Stage 1's fused scores — drive what survives
    // near-duplicate dropping and greedy packing below. Skipped
    // entirely when no reranker is configured.
    let candidates = match &request.reranker {
        Some(reranker) => crate::rerank::rerank(reranker.as_ref(), &request.query, candidates)?,
        None => candidates,
    };

    // Deduplicate: drop any candidate whose text is a near-duplicate
    // of an already-kept, higher-ranked candidate.
    let mut kept: Vec<SearchHit> = Vec::new();
    let mut kept_word_sets: Vec<HashSet<String>> = Vec::new();
    for candidate in candidates {
        let words = word_set(&candidate.text);
        let is_duplicate = kept_word_sets.iter().any(|existing| jaccard(existing, &words) >= NEAR_DUPLICATE_THRESHOLD);
        if !is_duplicate {
            kept_word_sets.push(words);
            kept.push(candidate);
        }
    }

    // Greedily pack by score order: take a candidate if it fits the
    // remaining token budget AND either its source is already
    // included or the distinct-source cap hasn't been hit yet.
    let mut packed: Vec<ContextChunk> = Vec::new();
    let mut tokens_used = 0usize;
    let mut sources_seen: HashSet<mnemo_core::ids::SourceId> = HashSet::new();
    // Message hits (no `source_id`) don't count against `max_sources`
    // — they're conversation history, not a document source pool.

    for hit in kept {
        let tokens = estimate_tokens(&hit.text);
        if tokens_used + tokens > request.token_budget {
            continue;
        }
        if let Some(source_id) = hit.source_id {
            let would_add_new_source = !sources_seen.contains(&source_id);
            if would_add_new_source && sources_seen.len() >= request.max_sources {
                continue;
            }
            sources_seen.insert(source_id);
        }
        tokens_used += tokens;
        packed.push(ContextChunk {
            hit,
            estimated_tokens: tokens,
        });
    }

    let conn = db.conn();

    // Neighbor-chunk expansion (plan.md's "preserve surrounding
    // context where needed"): opt-in post-pack step, see
    // `expand_with_neighbors` for why it runs after rather than
    // during the main greedy pack.
    let packed = if request.neighbor_expansion {
        expand_with_neighbors(&conn, packed, &mut tokens_used, request.token_budget)?
    } else {
        packed
    };

    // Hydrate full `Source` records for citation rendering, in
    // first-selected order.
    let mut source_cache: HashMap<mnemo_core::ids::SourceId, Source> = HashMap::new();
    let mut ordered_sources = Vec::new();
    for chunk in &packed {
        let Some(source_id) = chunk.hit.source_id else { continue };
        if source_cache.contains_key(&source_id) {
            continue;
        }
        if let Ok(source) = sources::get(&conn, source_id) {
            source_cache.insert(source_id, source.clone());
            ordered_sources.push(source);
        }
    }

    Ok(PackedContext {
        estimated_tokens: tokens_used,
        chunks: packed,
        sources: ordered_sources,
    })
}

/// Post-pack step for [`ContextRequest::neighbor_expansion`]: for
/// each already-selected document chunk, try to pull in its
/// immediate `chunk_index - 1` / `chunk_index + 1` siblings from the
/// same document.
///
/// This runs as a separate pass *after* the main greedy pack rather
/// than being folded into it, because neighbor lookups key off a
/// chunk's `document_id`/`chunk_index` — fields that live on the
/// stored [`mnemo_core::models::Chunk`], not on [`SearchHit`] — so
/// they can only be resolved for chunks the main pack already
/// decided to select (`hybrid_search`'s candidate pool has no
/// business paying for that lookup on every candidate, most of which
/// won't make the cut). It also keeps `neighbor_expansion` purely
/// additive: budget/diversity decisions made by the main pack are
/// never revisited, only supplemented.
///
/// Neighbors inherit their parent's provenance (`document_title`,
/// `source_id`, `source_name`) — they're always in the same document
/// as the chunk they're expanding — but their own `section`/`page`,
/// since either can legitimately differ from the parent's within a
/// document. They also inherit the parent's `score` rather than
/// getting one of their own: they were never independently ranked,
/// only pulled in for continuity around a chunk that was.
fn expand_with_neighbors(
    conn: &Connection,
    packed: Vec<ContextChunk>,
    tokens_used: &mut usize,
    token_budget: usize,
) -> Result<Vec<ContextChunk>> {
    let mut included: HashSet<ChunkId> = packed.iter().filter_map(|c| c.hit.chunk_id).collect();
    let mut expanded: Vec<ContextChunk> = Vec::with_capacity(packed.len());

    for chunk in packed {
        // Only document chunks have a `chunk_index`/`document_id` to
        // look up siblings for; conversation message hits pass
        // through untouched.
        let Some(chunk_id) = (chunk.hit.kind == HitKind::Chunk).then(|| chunk.hit.chunk_id).flatten() else {
            expanded.push(chunk);
            continue;
        };
        let Ok(record) = chunks::get(conn, chunk_id) else {
            // The chunk backing this hit vanished between search and
            // packing (e.g. concurrent deletion) — keep the hit as
            // selected, just skip expanding it.
            expanded.push(chunk);
            continue;
        };

        let prev = match record.chunk_index.checked_sub(1) {
            Some(prev_index) => {
                try_add_neighbor(conn, &chunk.hit, record.document_id, prev_index, &mut included, tokens_used, token_budget)?
            }
            None => None,
        };
        let next = try_add_neighbor(
            conn,
            &chunk.hit,
            record.document_id,
            record.chunk_index + 1,
            &mut included,
            tokens_used,
            token_budget,
        )?;

        if let Some(prev) = prev {
            expanded.push(prev);
        }
        expanded.push(chunk);
        if let Some(next) = next {
            expanded.push(next);
        }
    }

    Ok(expanded)
}

/// Look up the chunk at `neighbor_index` in `document_id` and, if it
/// exists, isn't already included, and fits the remaining
/// `token_budget`, build a [`ContextChunk`] for it and reserve its
/// tokens against `tokens_used`. Returns `Ok(None)` for any reason
/// the neighbor can't be added — that's the normal, expected outcome
/// at either edge of a document or once the budget is exhausted, not
/// an error condition.
fn try_add_neighbor(
    conn: &Connection,
    parent_hit: &SearchHit,
    document_id: mnemo_core::ids::DocumentId,
    neighbor_index: usize,
    included: &mut HashSet<ChunkId>,
    tokens_used: &mut usize,
    token_budget: usize,
) -> Result<Option<ContextChunk>> {
    let Some(neighbor) = chunks::get_by_document_and_index(conn, document_id, neighbor_index)? else {
        return Ok(None);
    };
    if included.contains(&neighbor.id) {
        return Ok(None);
    }
    let tokens = estimate_tokens(&neighbor.text);
    if *tokens_used + tokens > token_budget {
        return Ok(None);
    }

    included.insert(neighbor.id);
    *tokens_used += tokens;
    Ok(Some(ContextChunk {
        hit: SearchHit {
            kind: HitKind::Chunk,
            text: neighbor.text,
            score: parent_hit.score,
            chunk_id: Some(neighbor.id),
            message_id: None,
            conversation_id: None,
            document_title: parent_hit.document_title.clone(),
            source_name: parent_hit.source_name.clone(),
            source_id: parent_hit.source_id,
            section: neighbor.section,
            page: neighbor.page,
        },
        estimated_tokens: tokens,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::models::{Chunk, Document, Embedding, SourceType};
    use mnemo_embeddings::{Embedder, HashingEmbedder};
    use mnemo_storage::repositories::{chunks, documents, embeddings, sources};

    /// Seed one source/document/chunk per entry in `chunk_texts`
    /// (so each chunk has a distinct source for diversity tests),
    /// each embedded with `embedder`.
    fn seed_multi_source(chunk_texts: &[&str], embedder: &dyn Embedder) -> Db {
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();

        for (i, text) in chunk_texts.iter().enumerate() {
            let source = Source::new(SourceType::File, format!("doc-{i}.txt"));
            sources::insert(&conn, &source).unwrap();
            let document = Document::new(source.id, "text/plain", format!("hash-{i}"), "v1");
            documents::insert(&conn, &document).unwrap();
            let chunk = Chunk::new(document.id, *text, 0);
            chunks::insert(&conn, &chunk).unwrap();
            let vector = embedder.embed(text).unwrap();
            let embedding = Embedding::new(chunk.id, embedder.model_name(), embedder.model_version(), vector);
            embeddings::upsert(&conn, &embedding).unwrap();
        }

        drop(conn);
        db
    }

    /// Seed a single source/document made up of `chunk_texts` in
    /// order (`chunk_index` 0, 1, 2, ...), each embedded with
    /// `embedder`. Unlike `seed_multi_source`, every chunk shares one
    /// document — what neighbor-chunk expansion needs to find
    /// `chunk_index - 1` / `chunk_index + 1` siblings for.
    fn seed_single_document(chunk_texts: &[&str], embedder: &dyn Embedder) -> Db {
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();

        let source = Source::new(SourceType::File, "doc.txt");
        sources::insert(&conn, &source).unwrap();
        let document = Document::new(source.id, "text/plain", "hash", "v1");
        documents::insert(&conn, &document).unwrap();

        for (i, text) in chunk_texts.iter().enumerate() {
            let chunk = Chunk::new(document.id, *text, i);
            chunks::insert(&conn, &chunk).unwrap();
            let vector = embedder.embed(text).unwrap();
            let embedding = Embedding::new(chunk.id, embedder.model_name(), embedder.model_version(), vector);
            embeddings::upsert(&conn, &embedding).unwrap();
        }

        drop(conn);
        db
    }

    #[test]
    fn estimate_tokens_is_roughly_chars_over_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn jaccard_word_set_similarity_detects_near_duplicates() {
        let a = word_set("the quick brown fox jumps");
        let b = word_set("the quick brown fox leaps");
        let c = word_set("completely unrelated text here");
        assert!(jaccard(&a, &b) >= NEAR_DUPLICATE_THRESHOLD);
        assert!(jaccard(&a, &c) < NEAR_DUPLICATE_THRESHOLD);
    }

    #[test]
    fn pack_context_respects_token_budget() {
        let embedder = HashingEmbedder::default_dim();
        let long_text = "rust programming language systems ".repeat(30);
        let db = seed_multi_source(&[&long_text, "rust programming is fun"], &embedder);

        let request = ContextRequest {
            query: "rust programming".to_string(),
            token_budget: 20,
            ..Default::default()
        };
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert!(packed.estimated_tokens <= 20);
        for chunk in &packed.chunks {
            assert!(chunk.estimated_tokens <= 20);
        }
    }

    #[test]
    fn pack_context_caps_distinct_sources() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_multi_source(
            &[
                "rust programming language one",
                "rust programming language two",
                "rust programming language three",
            ],
            &embedder,
        );

        let request = ContextRequest {
            query: "rust programming language".to_string(),
            token_budget: 10_000,
            max_sources: 2,
            ..Default::default()
        };
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert!(packed.sources.len() <= 2);
    }

    #[test]
    fn pack_context_drops_near_duplicate_chunks() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_multi_source(
            &["rust programming is great for systems", "rust programming is great for systems!"],
            &embedder,
        );

        let request = ContextRequest {
            query: "rust programming".to_string(),
            token_budget: 10_000,
            ..Default::default()
        };
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert_eq!(packed.chunks.len(), 1);
    }

    /// A reranker that ignores every signal on `SearchHit` and scores
    /// purely by document title, so this test doesn't depend on how
    /// `hybrid_search`'s fused scores (or `HeuristicReranker`'s
    /// min-max normalization of just two of them, which always maps
    /// to the extremes `0.0`/`1.0`) happen to order the two seeded
    /// chunks — only on whether `pack_context` actually invoked the
    /// configured reranker at all.
    struct TitleOnlyReranker;
    impl Reranker for TitleOnlyReranker {
        fn name(&self) -> &str {
            "title-only"
        }
        fn score(&self, _query: &str, hits: &[SearchHit]) -> Result<Vec<f64>> {
            Ok(hits
                .iter()
                .map(|h| if h.document_title.as_deref() == Some("Rust Programming Guide") { 1.0 } else { 0.0 })
                .collect())
        }
    }

    #[test]
    fn pack_context_applies_configured_reranker() {
        let embedder = HashingEmbedder::default_dim();
        // Two otherwise-equivalent chunks in different documents;
        // `TitleOnlyReranker` always ranks the "Rust Programming
        // Guide" one first, so seeing it first in `packed.chunks`
        // proves `pack_context` actually ran Stage 2 reranking.
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();
        let titled_source = Source::new(SourceType::File, "rust-guide.txt");
        sources::insert(&conn, &titled_source).unwrap();
        let mut titled_doc = Document::new(titled_source.id, "text/plain", "hash-0", "v1");
        titled_doc.title = Some("Rust Programming Guide".to_string());
        documents::insert(&conn, &titled_doc).unwrap();
        let titled_chunk = Chunk::new(titled_doc.id, "some unrelated body text here", 0);
        chunks::insert(&conn, &titled_chunk).unwrap();
        let titled_vector = embedder.embed("some unrelated body text here").unwrap();
        embeddings::upsert(
            &conn,
            &Embedding::new(titled_chunk.id, embedder.model_name(), embedder.model_version(), titled_vector),
        )
        .unwrap();

        let plain_source = Source::new(SourceType::File, "cooking.txt");
        sources::insert(&conn, &plain_source).unwrap();
        let mut plain_doc = Document::new(plain_source.id, "text/plain", "hash-1", "v1");
        plain_doc.title = Some("Cooking Guide".to_string());
        documents::insert(&conn, &plain_doc).unwrap();
        let plain_chunk = Chunk::new(plain_doc.id, "some unrelated body text here too", 0);
        chunks::insert(&conn, &plain_chunk).unwrap();
        let plain_vector = embedder.embed("some unrelated body text here too").unwrap();
        embeddings::upsert(
            &conn,
            &Embedding::new(plain_chunk.id, embedder.model_name(), embedder.model_version(), plain_vector),
        )
        .unwrap();
        drop(conn);

        let request = ContextRequest::new("rust programming", 10_000).with_reranker(Arc::new(TitleOnlyReranker));
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert_eq!(packed.chunks.len(), 2);
        assert_eq!(packed.chunks[0].hit.document_title.as_deref(), Some("Rust Programming Guide"));
    }

    #[test]
    fn pack_context_without_reranker_skips_stage_two() {
        // No `reranker` configured (the `Default`) — `pack_context`
        // must not error or alter behavior; this is a regression
        // guard for the `None` branch alongside the `Some` branch
        // exercised by `pack_context_applies_configured_reranker`.
        let embedder = HashingEmbedder::default_dim();
        let db = seed_multi_source(&["rust programming language one", "rust programming language two"], &embedder);
        let request = ContextRequest::new("rust programming", 10_000);
        assert!(request.reranker.is_none());
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert_eq!(packed.chunks.len(), 2);
    }

    #[test]
    fn pack_context_expands_neighbor_chunks_when_enabled() {
        let embedder = HashingEmbedder::default_dim();
        // `candidate_pool: 1` forces `hybrid_search` to return only
        // the single best-matching chunk, so chunk 0 and chunk 2
        // never enter the main pack on their own merit — only
        // neighbor expansion can bring them in.
        let db = seed_single_document(
            &[
                "completely unrelated filler content",
                "rust programming language guide chapter",
                "more filler content that follows",
            ],
            &embedder,
        );

        let request = ContextRequest {
            query: "rust programming language".to_string(),
            token_budget: 10_000,
            candidate_pool: 1,
            neighbor_expansion: true,
            ..Default::default()
        };
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert_eq!(packed.chunks.len(), 3);
        // Order follows document order around the originally-selected
        // chunk: previous neighbor, the chunk itself, next neighbor.
        assert_eq!(packed.chunks[0].hit.text, "completely unrelated filler content");
        assert_eq!(packed.chunks[1].hit.text, "rust programming language guide chapter");
        assert_eq!(packed.chunks[2].hit.text, "more filler content that follows");
    }

    #[test]
    fn pack_context_neighbor_expansion_respects_token_budget() {
        let embedder = HashingEmbedder::default_dim();
        let neighbor_text = "padding text shared by both neighbor chunks";
        let main_text = "rust programming language chapter core content";
        let db = seed_single_document(&[neighbor_text, main_text, neighbor_text], &embedder);

        // Budget for exactly the main chunk plus one identical-sized
        // neighbor — not both.
        let token_budget = estimate_tokens(main_text) + estimate_tokens(neighbor_text);

        let request = ContextRequest {
            query: "rust programming language".to_string(),
            token_budget,
            candidate_pool: 1,
            neighbor_expansion: true,
            ..Default::default()
        };
        let packed = pack_context(&db, &embedder, &request).unwrap();
        // The previous neighbor is tried before the next one (see
        // `expand_with_neighbors`), so it wins the remaining budget.
        assert_eq!(packed.chunks.len(), 2);
        assert_eq!(packed.chunks[0].hit.text, neighbor_text);
        assert_eq!(packed.chunks[1].hit.text, main_text);
        assert!(packed.estimated_tokens <= token_budget);
    }

    #[test]
    fn pack_context_neighbor_expansion_disabled_by_default() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_single_document(
            &["filler before", "rust programming language chapter", "filler after"],
            &embedder,
        );

        let request = ContextRequest {
            query: "rust programming language".to_string(),
            token_budget: 10_000,
            candidate_pool: 1,
            ..Default::default()
        };
        assert!(!request.neighbor_expansion);
        let packed = pack_context(&db, &embedder, &request).unwrap();
        assert_eq!(packed.chunks.len(), 1);
    }
}
