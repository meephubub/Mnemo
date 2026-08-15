//! Brute-force vector search over the `embeddings` table (plan.md
//! section 6 "Vector Storage" / Phase 4 deliverable "Semantic search
//! works locally").
//!
//! There is no ANN index yet — every call does a full cosine-
//! similarity scan over every stored vector for the given model. That
//! is fine at the scale a single local knowledge base reaches; a
//! sub-linear index is tracked in ROADMAP.md as future work and can
//! replace the body of [`vector_search`] without changing its
//! signature.

use mnemo_embeddings::Embedder;
use mnemo_storage::repositories::{documents, embeddings, sources};
use mnemo_storage::Db;

use crate::error::Result;
use crate::{HitKind, SearchHit};

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    // Embeddings produced by `mnemo-embeddings::Embedder` implementations
    // are L2-normalized, so cosine similarity reduces to a dot product.
    // Compute the full cosine formula anyway so this holds for any
    // future `Embedder` that doesn't normalize its output.
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Embed `query` with `embedder` and return the top `limit` chunks by
/// cosine similarity against every embedding stored for
/// `embedder`'s `(model_name, model_version)`.
pub fn vector_search(db: &Db, embedder: &dyn Embedder, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let query_vector = embedder
        .embed(query)
        .map_err(|e| crate::error::SearchError::Embedding(e.to_string()))?;

    let conn = db.conn();
    let candidates = embeddings::list_by_model(&conn, embedder.model_name(), embedder.model_version())?;

    let mut scored: Vec<(f64, mnemo_core::ids::ChunkId)> = candidates
        .into_iter()
        .map(|e| (cosine_similarity(&query_vector, &e.vector), e.chunk_id))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    let mut hits = Vec::with_capacity(scored.len());
    for (score, chunk_id) in scored {
        let chunk = match mnemo_storage::fts::get_chunk(&conn, chunk_id) {
            Ok(chunk) => chunk,
            Err(_) => continue,
        };
        let document = documents::get(&conn, chunk.document_id).ok();
        let source = document.as_ref().and_then(|d| sources::get(&conn, d.source_id).ok());

        hits.push(SearchHit {
            kind: HitKind::Chunk,
            text: chunk.text,
            score,
            chunk_id: Some(chunk.id),
            message_id: None,
            conversation_id: None,
            document_title: document.and_then(|d| d.title),
            source_id: source.as_ref().map(|s| s.id),
            source_name: source.map(|s| s.name),
            section: chunk.section,
            page: chunk.page,
        });
    }
    Ok(hits)
}
