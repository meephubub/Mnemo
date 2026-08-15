//! Hybrid retrieval: lexical (BM25) + vector (cosine) score fusion
//! (plan.md section 8 "Hybrid Retrieval" / Phase 5).
//!
//! This implements the "Possible initial score" from plan.md section
//! 8 restricted to its two implemented signals (semantic + lexical);
//! entity/recency/importance weights are placeholders for later
//! phases (12, 26, 27) and are not part of this fusion yet.
//!
//! Fusion strategy: min-max normalize each candidate list to `[0, 1]`
//! independently (so BM25's and cosine's very different scales don't
//! bias the result), then combine matching hits (chunk or message)
//! with a weighted sum. A hit that only one retriever found still
//! competes, using its normalized score from the list it appeared in
//! and `0.0` from the other.

use std::collections::HashMap;

use mnemo_core::ids::{ChunkId, MessageId};
use mnemo_embeddings::Embedder;
use mnemo_storage::Db;

use crate::error::Result;
use crate::{search, vector, HitKind, SearchHit, SearchOptions};

/// Fusion key: a hit's identity across the lexical and vector
/// candidate lists, regardless of whether it's a chunk or a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HitKey {
    Chunk(ChunkId),
    Message(MessageId),
}

fn hit_key(hit: &SearchHit) -> Option<HitKey> {
    match hit.kind {
        HitKind::Chunk => hit.chunk_id.map(HitKey::Chunk),
        HitKind::Message => hit.message_id.map(HitKey::Message),
    }
}

/// Weights applied to each retrieval signal during fusion. Both
/// default to `0.5`; the values don't need to sum to 1 — they're
/// relative weights, not a probability distribution.
#[derive(Debug, Clone, Copy)]
pub struct HybridWeights {
    pub lexical_weight: f64,
    pub vector_weight: f64,
}

impl Default for HybridWeights {
    fn default() -> Self {
        Self {
            lexical_weight: 0.5,
            vector_weight: 0.5,
        }
    }
}

fn min_max_normalize(hits: &[SearchHit]) -> Vec<f64> {
    if hits.is_empty() {
        return Vec::new();
    }
    let min = hits.iter().map(|h| h.score).fold(f64::INFINITY, f64::min);
    let max = hits.iter().map(|h| h.score).fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    hits.iter()
        .map(|h| if range > 0.0 { (h.score - min) / range } else { 1.0 })
        .collect()
}

/// Run lexical and vector search independently, then fuse the
/// results into a single ranked list via weighted, min-max normalized
/// score fusion.
///
/// Both chunk and conversation-message hits participate in vector
/// fusion (messages are embedded the same way chunks are — see
/// [`crate::vector::vector_search`]). `vector_search` itself doesn't
/// take a scope, so its results are filtered here to match
/// `options.scope` before fusing, the same way the lexical pass
/// already restricts itself to chunks/messages per scope.
pub fn hybrid_search(
    db: &Db,
    embedder: &dyn Embedder,
    query: &str,
    options: &SearchOptions,
    weights: HybridWeights,
) -> Result<Vec<SearchHit>> {
    // Cast a wider net than the final `limit` on each candidate list
    // so fusion has enough overlap to work with, per plan.md section
    // 10's "Stage 1: cheap candidate generation" pattern.
    let candidate_limit = (options.limit * 4).max(20);

    let lexical_options = SearchOptions {
        limit: candidate_limit,
        ..*options
    };
    let lexical_hits = search(db, query, &lexical_options)?;
    let vector_hits: Vec<SearchHit> = vector::vector_search(db, embedder, query, candidate_limit)?
        .into_iter()
        .filter(|hit| match (hit.kind, options.scope) {
            (_, crate::SearchScope::All) => true,
            (HitKind::Chunk, crate::SearchScope::Documents) => true,
            (HitKind::Message, crate::SearchScope::Conversations) => true,
            _ => false,
        })
        .collect();

    let lexical_norm = min_max_normalize(&lexical_hits);
    let vector_norm = min_max_normalize(&vector_hits);

    // Fuse hits (chunk or message) by identity.
    let mut fused: HashMap<HitKey, (f64, SearchHit)> = HashMap::new();

    for (hit, norm_score) in lexical_hits.into_iter().zip(lexical_norm) {
        let Some(key) = hit_key(&hit) else { continue };
        fused.insert(key, (norm_score * weights.lexical_weight, hit));
    }

    for (hit, norm_score) in vector_hits.into_iter().zip(vector_norm) {
        let Some(key) = hit_key(&hit) else { continue };
        let weighted = norm_score * weights.vector_weight;
        fused
            .entry(key)
            .and_modify(|(score, _)| *score += weighted)
            .or_insert_with(|| (weighted, hit));
    }

    let mut results: Vec<(f64, SearchHit)> = fused.into_values().collect();
    results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(options.limit);

    Ok(results
        .into_iter()
        .map(|(score, mut hit)| {
            hit.score = score;
            hit
        })
        .collect())
}
