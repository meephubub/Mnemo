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
//! bias the result), then combine matching chunks with a weighted
//! sum. A hit that only one retriever found still competes, using its
//! normalized score from the list it appeared in and `0.0` from the
//! other.

use std::collections::HashMap;

use mnemo_core::ids::ChunkId;
use mnemo_embeddings::Embedder;
use mnemo_storage::Db;

use crate::error::Result;
use crate::{search, vector, SearchHit, SearchOptions};

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
/// Only chunk hits (not conversation messages) participate in vector
/// fusion today — messages aren't embedded yet (see ROADMAP.md) — so
/// message hits from the lexical pass are appended after fused chunk
/// hits, each keeping its normalized lexical score.
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
    let vector_hits = vector::vector_search(db, embedder, query, candidate_limit)?;

    let lexical_norm = min_max_normalize(&lexical_hits);
    let vector_norm = min_max_normalize(&vector_hits);

    // Fuse chunk hits by id; carry lexical message hits through
    // untouched (normalized, lexical-weighted only).
    let mut fused: HashMap<ChunkId, (f64, SearchHit)> = HashMap::new();
    let mut message_hits: Vec<(f64, SearchHit)> = Vec::new();

    for (hit, norm_score) in lexical_hits.into_iter().zip(lexical_norm) {
        match hit.chunk_id {
            Some(id) => {
                fused.insert(id, (norm_score * weights.lexical_weight, hit));
            }
            None => message_hits.push((norm_score * weights.lexical_weight, hit)),
        }
    }

    for (hit, norm_score) in vector_hits.into_iter().zip(vector_norm) {
        let Some(id) = hit.chunk_id else { continue };
        let weighted = norm_score * weights.vector_weight;
        fused
            .entry(id)
            .and_modify(|(score, _)| *score += weighted)
            .or_insert_with(|| (weighted, hit));
    }

    let mut results: Vec<(f64, SearchHit)> = fused.into_values().chain(message_hits).collect();
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
