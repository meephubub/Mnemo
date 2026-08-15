//! `mnemo-search` — the retrieval engine.
//!
//! Implements:
//! - Phase 3 ("Full-Text Search", plan.md section 79): lexical/BM25
//!   search over chunks and conversation messages via the FTS5
//!   indexes in `mnemo-storage` ([`search`]).
//! - Phase 4 ("Embeddings", plan.md section 80, retrieval half):
//!   brute-force cosine similarity search over the `embeddings`
//!   table ([`vector::vector_search`]).
//! - Phase 5 ("Hybrid Retrieval", plan.md section 8 / section 81):
//!   weighted, min-max normalized fusion of the two above
//!   ([`hybrid::hybrid_search`]).
//! - Phase 7 ("Context Packing", plan.md section 11 / section 83):
//!   deduplicated, token-budgeted, source-diverse packing of hybrid
//!   candidates into a `PackedContext` ready to drop into a prompt
//!   ([`context::pack_context`]).
//! - Phase 6 ("Reranking", plan.md section 10 / section 82): an
//!   optional second stage over a fused candidate pool via the
//!   pluggable [`rerank::Reranker`] trait ([`rerank::rerank`]), with
//!   [`rerank::HeuristicReranker`] as the default, dependency-free
//!   implementation. Callers opt in explicitly — it is never forced
//!   into [`hybrid_search`], and [`context::pack_context`] only runs
//!   it when [`ContextRequest::reranker`] is set.
//!
//! Every search function here shares the same `SearchHit` shape
//! (with `score` meaning "higher is better" in every case) so a
//! caller can switch between them without touching downstream code.

pub mod context;
pub mod error;
pub mod hybrid;
pub mod rerank;
pub mod vector;

pub use context::{pack_context, ContextChunk, ContextRequest, PackedContext};
pub use error::{Result, SearchError};
pub use hybrid::{hybrid_search, HybridWeights};
pub use rerank::{rerank, HeuristicReranker, Reranker};
pub use vector::vector_search;

use mnemo_core::ids::{ChunkId, ConversationId, MessageId, SourceId};
use mnemo_storage::repositories::{documents, sources};
use mnemo_storage::Db;

/// Which part of the knowledge base to search (plan.md section 42
/// "Query Scopes"). `Profile`, `Emails`, `Project`, and `Entity`
/// scopes are planned but require features from later phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Documents,
    Conversations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Chunk,
    Message,
}

/// A single retrieval result with enough provenance to render a
/// citation (plan.md section 36 "Citation System").
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub kind: HitKind,
    pub text: String,
    /// Higher is better (raw FTS5 `bm25()` scores are negated here so
    /// every scoring stage in the pipeline shares the same direction).
    pub score: f64,
    pub chunk_id: Option<ChunkId>,
    pub message_id: Option<MessageId>,
    pub conversation_id: Option<ConversationId>,
    pub document_title: Option<String>,
    pub source_name: Option<String>,
    /// Identifies the owning [`mnemo_core::models::Source`], used by
    /// [`context::pack_context`] for source-diversity selection and to
    /// hydrate `PackedContext::sources`. `None` for conversation
    /// message hits, which aren't attached to a `Source` today.
    pub source_id: Option<SourceId>,
    pub section: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub scope: SearchScope,
    pub limit: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            scope: SearchScope::All,
            limit: 10,
        }
    }
}

/// Escape a free-text query for SQLite FTS5's `MATCH` syntax by
/// quoting every token, so user input containing FTS5 operators
/// (`-`, `"`, `*`, `:`, ...) can't produce a syntax error or an
/// unintended query.
pub fn sanitize_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a lexical search across the requested scope, hydrating each
/// hit with citation metadata.
pub fn search(db: &Db, query: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
    let fts_query = sanitize_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let conn = db.conn();
    let mut hits = Vec::new();

    if matches!(options.scope, SearchScope::All | SearchScope::Documents) {
        let chunk_hits = mnemo_storage::fts::search_chunks(&conn, &fts_query, options.limit)?;
        for hit in chunk_hits {
            let document = documents::get(&conn, hit.chunk.document_id).ok();
            let source = document
                .as_ref()
                .and_then(|d| sources::get(&conn, d.source_id).ok());

            hits.push(SearchHit {
                kind: HitKind::Chunk,
                text: hit.chunk.text,
                score: -hit.score,
                chunk_id: Some(hit.chunk.id),
                message_id: None,
                conversation_id: None,
                document_title: document.and_then(|d| d.title),
                source_id: source.as_ref().map(|s| s.id),
                source_name: source.map(|s| s.name),
                section: hit.chunk.section,
                page: hit.chunk.page,
            });
        }
    }

    if matches!(options.scope, SearchScope::All | SearchScope::Conversations) {
        let message_hits = mnemo_storage::fts::search_messages(&conn, &fts_query, options.limit)?;
        for hit in message_hits {
            hits.push(SearchHit {
                kind: HitKind::Message,
                text: hit.message.content,
                score: -hit.score,
                chunk_id: None,
                message_id: Some(hit.message.id),
                conversation_id: Some(hit.message.conversation_id),
                document_title: None,
                source_name: None,
                source_id: None,
                section: None,
                page: None,
            });
        }
    }

    // Merge-sort the two candidate streams by score (highest first)
    // and truncate. Score fusion across signal *types* (vector,
    // entity, recency, ...) arrives in Phase 5 "Hybrid Retrieval".
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(options.limit);

    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::models::{Chunk, Conversation, Document, Embedding, Message, MessageEmbedding, MessageRole, Source, SourceType};
    use mnemo_embeddings::{Embedder, HashingEmbedder};
    use mnemo_storage::repositories::{chunks, conversations, documents, embeddings, message_embeddings, sources};

    /// Seed an in-memory db with one document made of a few chunks,
    /// and embed each chunk with `embedder`. Returns the db.
    fn seed(chunk_texts: &[&str], embedder: &dyn Embedder) -> Db {
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();

        let source = Source::new(SourceType::File, "test.txt");
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
    fn lexical_search_finds_matching_chunk() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed(
            &["the rust programming language is fast", "french cooking recipes"],
            &embedder,
        );

        let hits = search(&db, "rust programming", &SearchOptions::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("rust"));
    }

    #[test]
    fn vector_search_ranks_semantically_closer_chunk_first() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed(
            &["the rust programming language is fast", "french cooking recipes are delicious"],
            &embedder,
        );

        let hits = vector::vector_search(&db, &embedder, "rust programming language", 10).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].text.contains("rust"));
    }

    #[test]
    fn hybrid_search_fuses_lexical_and_vector_results() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed(
            &["the rust programming language is fast", "french cooking recipes are delicious"],
            &embedder,
        );

        let hits = hybrid::hybrid_search(
            &db,
            &embedder,
            "rust programming",
            &SearchOptions::default(),
            HybridWeights::default(),
        )
        .unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].text.contains("rust"));
    }

    #[test]
    fn vector_search_on_empty_query_returns_no_hits() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed(&["some text"], &embedder);
        let hits = vector::vector_search(&db, &embedder, "   ", 10).unwrap();
        assert!(hits.is_empty());
    }

    /// Seed an in-memory db with one conversation made of a few
    /// messages, embedding each one with `embedder` into
    /// `message_embeddings` (Phase 8 follow-up — see ROADMAP.md).
    fn seed_messages(message_texts: &[&str], embedder: &dyn Embedder) -> Db {
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();

        let conversation = Conversation::new(None);
        conversations::insert_conversation(&conn, &conversation).unwrap();

        for text in message_texts {
            let message = Message::new(conversation.id, MessageRole::User, *text);
            conversations::insert_message(&conn, &message).unwrap();

            let vector = embedder.embed(text).unwrap();
            let embedding = MessageEmbedding::new(message.id, embedder.model_name(), embedder.model_version(), vector);
            message_embeddings::upsert(&conn, &embedding).unwrap();
        }

        drop(conn);
        db
    }

    #[test]
    fn vector_search_ranks_semantically_closer_message_first() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_messages(
            &["the rust programming language is fast", "french cooking recipes are delicious"],
            &embedder,
        );

        let hits = vector::vector_search(&db, &embedder, "rust programming language", 10).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].kind, HitKind::Message);
        assert!(hits[0].text.contains("rust"));
        assert!(hits[0].message_id.is_some());
        assert!(hits[0].conversation_id.is_some());
    }

    #[test]
    fn hybrid_search_fuses_message_vector_hit_with_lexical_hit() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_messages(
            &["the rust programming language is fast", "french cooking recipes are delicious"],
            &embedder,
        );

        let hits = hybrid::hybrid_search(
            &db,
            &embedder,
            "rust programming",
            &SearchOptions::default(),
            HybridWeights::default(),
        )
        .unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].kind, HitKind::Message);
        assert!(hits[0].text.contains("rust"));
    }

    #[test]
    fn hybrid_search_documents_scope_excludes_message_vector_hits() {
        let embedder = HashingEmbedder::default_dim();
        let db = seed_messages(&["the rust programming language is fast"], &embedder);

        let hits = hybrid::hybrid_search(
            &db,
            &embedder,
            "rust programming",
            &SearchOptions {
                scope: SearchScope::Documents,
                limit: 10,
            },
            HybridWeights::default(),
        )
        .unwrap();
        assert!(hits.is_empty());
    }
}
