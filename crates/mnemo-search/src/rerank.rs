//! Reranking: an optional second stage over a fused candidate pool
//! (plan.md section 10 "Reranking" / section 82 "Phase 6").
//!
//! plan.md sketches this as a two-stage pipeline — cheap candidate
//! generation (Stage 1, ~50 candidates, already `hybrid_search`) then
//! "expensive reranking" (Stage 2, down to ~5-15) via a pluggable
//! `Reranker`:
//!
//! ```text
//! trait Reranker {
//!     async fn rerank(&self, query: &str, documents: &[Document]) -> Result<Vec<RankedDocument>>;
//! }
//! ```
//!
//! This module defines that abstraction as [`Reranker`], adapted to
//! this crate's synchronous, `SearchHit`-based pipeline (the facade's
//! `blocking::run` wrapper is what supplies async at the API
//! boundary — see `mnemo-search`'s and the facade's other stages).
//! [`HeuristicReranker`] is the default, dependency-free
//! implementation: no model download or inference runtime, matching
//! [`mnemo_embeddings::HashingEmbedder`]'s role as a "works
//! everywhere, swap in a real model later" baseline. plan.md's own
//! candidate models (BGE/Jina rerankers, other ONNX cross-encoders)
//! can implement the same [`Reranker`] trait without changing
//! [`rerank`] or its callers.
//!
//! Per plan.md ("The reranker should be optional" / "Allow reranking
//! to be disabled for low-latency queries"), reranking is never
//! forced into [`crate::hybrid_search`] or [`crate::pack_context`] —
//! callers opt in by calling [`rerank`] themselves on a candidate
//! pool, e.g. before packing.

use crate::error::Result;
use crate::SearchHit;

/// A pluggable reranking model or heuristic. Scores an existing
/// candidate pool against `query`; does not generate new candidates.
///
/// Implementations should be deterministic for the same `(query,
/// hits)` pair, mirroring the determinism requirement on
/// [`mnemo_embeddings::Embedder`].
pub trait Reranker: Send + Sync {
    /// Human-readable identifier for logging/debugging (e.g.
    /// `"heuristic"` or `"bge-reranker-base"`).
    fn name(&self) -> &str;

    /// Re-score `hits` against `query`. Implementations may reorder
    /// freely; [`rerank`] takes care of sorting by the returned
    /// scores and does not assume input order is preserved.
    ///
    /// Returns one score per input hit, in the same order as `hits`
    /// (not sorted) — [`rerank`] pairs `scores[i]` with `hits[i]`
    /// itself so implementations don't need to thread hits through
    /// their own return type.
    fn score(&self, query: &str, hits: &[SearchHit]) -> Result<Vec<f64>>;
}

/// Rerank `hits` against `query` using `reranker`, returning a new
/// vec sorted by the reranker's scores (highest first) with each
/// hit's `score` field overwritten to match — so downstream code
/// (e.g. [`crate::context::pack_context`]'s greedy packer) keeps
/// working against "higher `score` is better" without caring whether
/// reranking ran.
///
/// This is Stage 2 of plan.md section 10's two-stage pipeline; `hits`
/// is expected to already be a Stage 1 candidate pool (e.g. from
/// [`crate::hybrid_search`]), not a full corpus scan — reranking
/// every stored chunk against every query defeats the "cheap
/// candidate generation, then expensive reranking over a shortlist"
/// point of having two stages at all.
pub fn rerank(reranker: &dyn Reranker, query: &str, hits: Vec<SearchHit>) -> Result<Vec<SearchHit>> {
    if hits.is_empty() {
        return Ok(hits);
    }

    let scores = reranker.score(query, &hits)?;
    if scores.len() != hits.len() {
        return Err(crate::error::SearchError::Rerank(format!(
            "reranker {:?} returned {} scores for {} hits",
            reranker.name(),
            scores.len(),
            hits.len()
        )));
    }

    let mut scored: Vec<(f64, SearchHit)> = scores.into_iter().zip(hits).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    Ok(scored
        .into_iter()
        .map(|(score, mut hit)| {
            hit.score = score;
            hit
        })
        .collect())
}

/// A dependency-free reranker built from lexical/structural signals
/// already available on [`SearchHit`], rather than a learned
/// cross-encoder model:
///
/// - Exact-phrase overlap: what fraction of the query's tokens appear
///   verbatim in the hit's text (case-insensitive). This is the same
///   kind of literal-match signal a cross-encoder partially learns,
///   computed directly instead of inferred.
/// - Title/section match boost: a small bonus when the query's terms
///   also appear in `document_title` or `section`, since a hit whose
///   *heading* matches the query is usually more relevant than one
///   that only matches deep in body text.
/// - The hit's incoming fused `score` (from whichever stage produced
///   it — lexical, vector, or hybrid) is blended in rather than
///   discarded, so reranking refines Stage 1's ranking instead of
///   ignoring it outright.
///
/// This will not match a real cross-encoder's ability to judge
/// semantic relevance beyond token overlap — that's exactly the
/// tradeoff plan.md's "reranker should be optional" / pluggable
/// design anticipates. A real model (BGE/Jina/other ONNX
/// cross-encoder) can implement [`Reranker`] and be swapped in
/// without touching [`rerank`] or any caller.
pub struct HeuristicReranker {
    /// Weight on the incoming Stage 1 score (min-max normalized
    /// before blending, so it's on the same `[0, 1]` scale as the
    /// other signals below).
    pub base_score_weight: f64,
    /// Weight on query/body exact-phrase token overlap.
    pub overlap_weight: f64,
    /// Weight on the title/section match boost.
    pub title_match_weight: f64,
}

impl Default for HeuristicReranker {
    fn default() -> Self {
        Self {
            base_score_weight: 0.4,
            overlap_weight: 0.4,
            title_match_weight: 0.2,
        }
    }
}

impl HeuristicReranker {
    pub fn new(base_score_weight: f64, overlap_weight: f64, title_match_weight: f64) -> Self {
        Self {
            base_score_weight,
            overlap_weight,
            title_match_weight,
        }
    }
}

fn query_tokens(query: &str) -> Vec<String> {
    query.split_whitespace().map(|t| t.to_lowercase()).collect()
}

/// Fraction of `tokens` that appear as a substring of `haystack`
/// (already lowercased by the caller).
fn token_overlap(tokens: &[String], haystack: &str) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let matched = tokens.iter().filter(|t| haystack.contains(t.as_str())).count();
    matched as f64 / tokens.len() as f64
}

fn min_max_normalize(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    values
        .iter()
        .map(|v| if range > 0.0 { (v - min) / range } else { 1.0 })
        .collect()
}

impl Reranker for HeuristicReranker {
    fn name(&self) -> &str {
        "heuristic"
    }

    fn score(&self, query: &str, hits: &[SearchHit]) -> Result<Vec<f64>> {
        let tokens = query_tokens(query);
        let base_scores: Vec<f64> = hits.iter().map(|h| h.score).collect();
        let base_norm = min_max_normalize(&base_scores);

        Ok(hits
            .iter()
            .zip(base_norm)
            .map(|(hit, base)| {
                let body_overlap = token_overlap(&tokens, &hit.text.to_lowercase());

                let title_haystack = [hit.document_title.as_deref(), hit.section.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                let title_match = if title_haystack.is_empty() {
                    0.0
                } else {
                    token_overlap(&tokens, &title_haystack)
                };

                self.base_score_weight * base
                    + self.overlap_weight * body_overlap
                    + self.title_match_weight * title_match
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HitKind;

    fn hit(text: &str, score: f64, title: Option<&str>) -> SearchHit {
        SearchHit {
            kind: HitKind::Chunk,
            text: text.to_string(),
            score,
            chunk_id: None,
            message_id: None,
            conversation_id: None,
            document_title: title.map(str::to_string),
            source_id: None,
            source_name: None,
            section: None,
            page: None,
        }
    }

    #[test]
    fn rerank_reorders_by_reranker_score() {
        let hits = vec![
            hit("french cooking recipes are delicious", 1.0, None),
            hit("the rust programming language is fast", 0.1, None),
        ];
        let reranker = HeuristicReranker::default();
        let reranked = rerank(&reranker, "rust programming language", hits).unwrap();
        assert!(reranked[0].text.contains("rust"));
        // Ranking is now driven by the reranker's own score, not the
        // stale Stage-1 score order.
        assert!(reranked[0].score >= reranked[1].score);
    }

    #[test]
    fn rerank_boosts_title_matches() {
        let hits = vec![
            hit("some unrelated body text here", 0.5, Some("Rust Programming Guide")),
            hit("some unrelated body text here too", 0.5, Some("Cooking Guide")),
        ];
        let reranker = HeuristicReranker::default();
        let reranked = rerank(&reranker, "rust programming", hits).unwrap();
        assert_eq!(reranked[0].document_title.as_deref(), Some("Rust Programming Guide"));
    }

    #[test]
    fn rerank_on_empty_hits_is_a_noop() {
        let reranker = HeuristicReranker::default();
        let reranked = rerank(&reranker, "anything", Vec::new()).unwrap();
        assert!(reranked.is_empty());
    }

    #[test]
    fn mismatched_score_count_is_an_error() {
        struct BrokenReranker;
        impl Reranker for BrokenReranker {
            fn name(&self) -> &str {
                "broken"
            }
            fn score(&self, _query: &str, _hits: &[SearchHit]) -> Result<Vec<f64>> {
                Ok(vec![1.0]) // wrong length on purpose
            }
        }
        let hits = vec![hit("a", 1.0, None), hit("b", 1.0, None)];
        let result = rerank(&BrokenReranker, "query", hits);
        assert!(result.is_err());
    }
}
