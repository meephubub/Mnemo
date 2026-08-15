//! Brute-force vector search over the `embeddings` and
//! `message_embeddings` tables (plan.md section 6 "Vector Storage" /
//! Phase 4 deliverable "Semantic search works locally"; message
//! coverage is a Phase 8 follow-up — see ROADMAP.md).
//!
//! There is no ANN index yet — every call does a full cosine-
//! similarity scan over every stored vector for the given model, for
//! both chunks and messages. That is fine at the scale a single local
//! knowledge base reaches; a sub-linear index is tracked in
//! ROADMAP.md as future work and can replace the body of
//! [`vector_search`] without changing its signature.

use mnemo_embeddings::Embedder;
use mnemo_storage::repositories::{documents, embeddings, message_embeddings, sources};
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

/// A candidate id from either embeddings table, kept alongside its
/// cosine score until both pools have been merged and ranked.
enum Candidate {
    Chunk(mnemo_core::ids::ChunkId),
    Message(mnemo_core::ids::MessageId),
}

/// Embed `query` with `embedder` and return the top `limit` chunks
/// and/or messages by cosine similarity against every embedding
/// stored for `embedder`'s `(model_name, model_version)`, across both
/// the `embeddings` (chunk) and `message_embeddings` tables.
pub fn vector_search(db: &Db, embedder: &dyn Embedder, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let query_vector = embedder
        .embed(query)
        .map_err(|e| crate::error::SearchError::Embedding(e.to_string()))?;

    let conn = db.conn();
    let chunk_candidates = embeddings::list_by_model(&conn, embedder.model_name(), embedder.model_version())?;
    let message_candidates =
        message_embeddings::list_by_model(&conn, embedder.model_name(), embedder.model_version())?;

    let mut scored: Vec<(f64, Candidate)> = chunk_candidates
        .into_iter()
        .map(|e| (cosine_similarity(&query_vector, &e.vector), Candidate::Chunk(e.chunk_id)))
        .chain(
            message_candidates
                .into_iter()
                .map(|e| (cosine_similarity(&query_vector, &e.vector), Candidate::Message(e.message_id))),
        )
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    let mut hits = Vec::with_capacity(scored.len());
    for (score, candidate) in scored {
        match candidate {
            Candidate::Chunk(chunk_id) => {
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
            Candidate::Message(message_id) => {
                let message = match mnemo_storage::fts::get_message(&conn, message_id) {
                    Ok(message) => message,
                    Err(_) => continue,
                };
                hits.push(SearchHit {
                    kind: HitKind::Message,
                    text: message.content,
                    score,
                    chunk_id: None,
                    message_id: Some(message.id),
                    conversation_id: Some(message.conversation_id),
                    document_title: None,
                    source_id: None,
                    source_name: None,
                    section: None,
                    page: None,
                });
            }
        }
    }
    Ok(hits)
}
