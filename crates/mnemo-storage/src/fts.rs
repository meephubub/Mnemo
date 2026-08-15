//! Lexical (BM25) search over the FTS5 indexes created in
//! [`crate::migrations`] (plan.md section 7 "Full-Text Search").
//!
//! This module is deliberately thin: it runs the FTS5 `MATCH` query
//! and joins back to the owning row, returning a score alongside the
//! full model so callers (e.g. `mnemo-search`) can fuse it with other
//! signals without re-querying.

use mnemo_core::ids::{ChunkId, DocumentId};
use mnemo_core::models::{Chunk, Message};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::repositories::chunks::from_row as chunk_from_row;
use crate::repositories::conversations::message_from_row;

/// A chunk matched by full-text search, with its BM25 score.
///
/// Lower `score` is a better match (raw SQLite `bm25()` output); callers
/// that want "higher is better" should negate it during fusion.
#[derive(Debug, Clone)]
pub struct ChunkHit {
    pub chunk: Chunk,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct MessageHit {
    pub message: Message,
    pub score: f64,
}

fn chunk_hit_from_row(row: &Row) -> rusqlite::Result<ChunkHit> {
    Ok(ChunkHit {
        chunk: chunk_from_row(row)?,
        score: row.get("score")?,
    })
}

fn message_hit_from_row(row: &Row) -> rusqlite::Result<MessageHit> {
    Ok(MessageHit {
        message: message_from_row(row)?,
        score: row.get("score")?,
    })
}

/// Full-text search over chunk text, optionally scoped to a single
/// document.
pub fn search_chunks(conn: &Connection, query: &str, limit: usize) -> Result<Vec<ChunkHit>> {
    let mut stmt = conn.prepare(
        "SELECT c.*, bm25(chunks_fts) AS score
         FROM chunks_fts
         JOIN chunks c ON c.rowid = chunks_fts.rowid
         WHERE chunks_fts.text MATCH ?1
         ORDER BY score ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![query, limit as i64], chunk_hit_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn search_chunks_in_document(
    conn: &Connection,
    query: &str,
    document_id: DocumentId,
    limit: usize,
) -> Result<Vec<ChunkHit>> {
    let mut stmt = conn.prepare(
        "SELECT c.*, bm25(chunks_fts) AS score
         FROM chunks_fts
         JOIN chunks c ON c.rowid = chunks_fts.rowid
         WHERE chunks_fts.text MATCH ?1 AND c.document_id = ?2
         ORDER BY score ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![query, document_id.to_string(), limit as i64],
        chunk_hit_from_row,
    )?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Full-text search over conversation message content.
pub fn search_messages(conn: &Connection, query: &str, limit: usize) -> Result<Vec<MessageHit>> {
    let mut stmt = conn.prepare(
        "SELECT m.*, bm25(messages_fts) AS score
         FROM messages_fts
         JOIN messages m ON m.rowid = messages_fts.rowid
         WHERE messages_fts.content MATCH ?1
         ORDER BY score ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![query, limit as i64], message_hit_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Fetch a single chunk by id (helper used after a hybrid candidate
/// pool has already selected which ids to hydrate).
pub fn get_chunk(conn: &Connection, id: ChunkId) -> Result<Chunk> {
    crate::repositories::chunks::get(conn, id)
}
