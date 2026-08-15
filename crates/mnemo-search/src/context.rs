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
//! **Not implemented yet:** "preserve surrounding context where
//! needed" (plan.md's neighbor-chunk expansion) — see ROADMAP.md.

use std::collections::{HashMap, HashSet};

use mnemo_core::models::Source;
use mnemo_storage::repositories::sources;
use mnemo_storage::Db;
use mnemo_embeddings::Embedder;

use crate::error::Result;
use crate::{hybrid_search, HybridWeights, SearchHit, SearchOptions, SearchScope};

/// Input to [`pack_context`] (plan.md section 11's `ContextRequest`).
#[derive(Debug, Clone)]
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
        }
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

    // Hydrate full `Source` records for citation rendering, in
    // first-selected order.
    let conn = db.conn();
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
}
