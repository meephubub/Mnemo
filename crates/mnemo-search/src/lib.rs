//! `mnemo-search` — the retrieval engine.
//!
//! Phase 3 ("Full-Text Search", plan.md section 79) implementation:
//! lexical/BM25 search over chunks and conversation messages via the
//! FTS5 indexes in `mnemo-storage`.
//!
//! Vector search, hybrid score fusion, reranking, and context packing
//! (plan.md sections 6, 8, 10, 11 / Phases 4-7) are not implemented
//! yet — see ROADMAP.md. This crate's public API (`SearchScope`,
//! `SearchHit`, `search`) is designed so those phases can be added
//! without breaking callers: hybrid search will fill in the same
//! `SearchHit.score` field with a fused score instead of a raw BM25
//! score.

pub mod error;

pub use error::{Result, SearchError};

use mnemo_core::ids::{ChunkId, ConversationId, MessageId};
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
    pub section: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone)]
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
