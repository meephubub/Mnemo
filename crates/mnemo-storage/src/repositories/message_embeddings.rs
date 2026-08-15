//! Message embeddings repository (Phase 8 follow-up — see
//! ROADMAP.md).
//!
//! Mirrors `repositories::embeddings` exactly, but over the
//! `message_embeddings` table / keyed on `message_id` instead of
//! `chunk_id`. Retrieval over this table is brute-force cosine
//! similarity, same as chunk embeddings (`mnemo-search`'s
//! `vector_search`).

use mnemo_core::ids::{EmbeddingId, MessageId};
use mnemo_core::models::MessageEmbedding;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::{dt_to_str, str_to_dt};

fn from_row(row: &Row) -> rusqlite::Result<MessageEmbedding> {
    let id: String = row.get("id")?;
    let message_id: String = row.get("message_id")?;
    let created_at: String = row.get("created_at")?;
    let vector_json: String = row.get("vector")?;

    let vector: Vec<f32> = serde_json::from_str(&vector_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(MessageEmbedding {
        id: id.parse::<uuid::Uuid>().map(EmbeddingId::from_uuid).unwrap_or_default(),
        message_id: message_id
            .parse::<uuid::Uuid>()
            .map(MessageId::from_uuid)
            .unwrap_or_default(),
        model_name: row.get("model_name")?,
        model_version: row.get("model_version")?,
        dimension: row.get::<_, i64>("dimension")? as usize,
        vector,
        created_at: str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Insert a message embedding, or replace the existing one for the
/// same `(message_id, model_name, model_version)` triple.
pub fn upsert(conn: &Connection, embedding: &MessageEmbedding) -> Result<()> {
    let vector_json = serde_json::to_string(&embedding.vector).map_err(|e| StorageError::Decode(e.to_string()))?;
    conn.execute(
        "INSERT INTO message_embeddings (id, message_id, model_name, model_version, dimension, vector, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(message_id, model_name, model_version) DO UPDATE SET
             dimension = excluded.dimension,
             vector = excluded.vector,
             created_at = excluded.created_at",
        params![
            embedding.id.to_string(),
            embedding.message_id.to_string(),
            embedding.model_name,
            embedding.model_version,
            embedding.dimension as i64,
            vector_json,
            dt_to_str(embedding.created_at),
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, message_id: MessageId, model_name: &str, model_version: &str) -> Result<MessageEmbedding> {
    conn.query_row(
        "SELECT * FROM message_embeddings WHERE message_id = ?1 AND model_name = ?2 AND model_version = ?3",
        params![message_id.to_string(), model_name, model_version],
        from_row,
    )
    .optional()?
    .ok_or_else(|| StorageError::NotFound(format!("embedding for message {message_id}")))
}

/// All message embeddings for a given model (the candidate pool for
/// brute-force vector search).
pub fn list_by_model(conn: &Connection, model_name: &str, model_version: &str) -> Result<Vec<MessageEmbedding>> {
    let mut stmt = conn.prepare("SELECT * FROM message_embeddings WHERE model_name = ?1 AND model_version = ?2")?;
    let rows = stmt.query_map(params![model_name, model_version], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Message ids that don't yet have an embedding for the given model
/// (drives `embed_pending_messages`'s incremental behaviour).
pub fn list_pending_message_ids(conn: &Connection, model_name: &str, model_version: &str) -> Result<Vec<MessageId>> {
    let mut stmt = conn.prepare(
        "SELECT m.id FROM messages m
         LEFT JOIN message_embeddings e
           ON e.message_id = m.id AND e.model_name = ?1 AND e.model_version = ?2
         WHERE e.id IS NULL",
    )?;
    let rows = stmt.query_map(params![model_name, model_version], |row| {
        let id: String = row.get(0)?;
        Ok(id)
    })?;
    Ok(rows
        .filter_map(|r| r.ok())
        .filter_map(|s| s.parse::<uuid::Uuid>().ok())
        .map(MessageId::from_uuid)
        .collect())
}

pub fn count(conn: &Connection, model_name: &str, model_version: &str) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM message_embeddings WHERE model_name = ?1 AND model_version = ?2",
        params![model_name, model_version],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

pub fn count_pending(conn: &Connection, model_name: &str, model_version: &str) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages m
         LEFT JOIN message_embeddings e
           ON e.message_id = m.id AND e.model_name = ?1 AND e.model_version = ?2
         WHERE e.id IS NULL",
        params![model_name, model_version],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

/// Delete every message embedding for a given model (e.g. before
/// rebuilding after a model upgrade).
pub fn clear(conn: &Connection, model_name: &str, model_version: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM message_embeddings WHERE model_name = ?1 AND model_version = ?2",
        params![model_name, model_version],
    )?;
    Ok(())
}
