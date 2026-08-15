//! Embeddings repository (plan.md section 47 "Local Embedding
//! Models" / section 49 "Model Versioning" / Phase 4).
//!
//! Vectors are stored as JSON text (see `migrations.rs` for why) and
//! read back with `serde_json`. Retrieval over this table is
//! brute-force cosine similarity for now (`mnemo-search`'s
//! `vector_search`); an ANN index is future work noted in
//! ROADMAP.md.

use mnemo_core::ids::{ChunkId, EmbeddingId};
use mnemo_core::models::Embedding;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::{dt_to_str, str_to_dt};

fn from_row(row: &Row) -> rusqlite::Result<Embedding> {
    let id: String = row.get("id")?;
    let chunk_id: String = row.get("chunk_id")?;
    let created_at: String = row.get("created_at")?;
    let vector_json: String = row.get("vector")?;

    let vector: Vec<f32> = serde_json::from_str(&vector_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Embedding {
        id: id.parse::<uuid::Uuid>().map(EmbeddingId::from_uuid).unwrap_or_default(),
        chunk_id: chunk_id.parse::<uuid::Uuid>().map(ChunkId::from_uuid).unwrap_or_default(),
        model_name: row.get("model_name")?,
        model_version: row.get("model_version")?,
        dimension: row.get::<_, i64>("dimension")? as usize,
        vector,
        created_at: str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Insert an embedding, or replace the existing one for the same
/// `(chunk_id, model_name, model_version)` triple (e.g. re-embedding
/// after editing a chunk).
pub fn upsert(conn: &Connection, embedding: &Embedding) -> Result<()> {
    let vector_json = serde_json::to_string(&embedding.vector).map_err(|e| StorageError::Decode(e.to_string()))?;
    conn.execute(
        "INSERT INTO embeddings (id, chunk_id, model_name, model_version, dimension, vector, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(chunk_id, model_name, model_version) DO UPDATE SET
             dimension = excluded.dimension,
             vector = excluded.vector,
             created_at = excluded.created_at",
        params![
            embedding.id.to_string(),
            embedding.chunk_id.to_string(),
            embedding.model_name,
            embedding.model_version,
            embedding.dimension as i64,
            vector_json,
            dt_to_str(embedding.created_at),
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, chunk_id: ChunkId, model_name: &str, model_version: &str) -> Result<Embedding> {
    conn.query_row(
        "SELECT * FROM embeddings WHERE chunk_id = ?1 AND model_name = ?2 AND model_version = ?3",
        params![chunk_id.to_string(), model_name, model_version],
        from_row,
    )
    .optional()?
    .ok_or_else(|| StorageError::NotFound(format!("embedding for chunk {chunk_id}")))
}

/// All embeddings for a given model (the candidate pool for
/// brute-force vector search).
pub fn list_by_model(conn: &Connection, model_name: &str, model_version: &str) -> Result<Vec<Embedding>> {
    let mut stmt = conn.prepare("SELECT * FROM embeddings WHERE model_name = ?1 AND model_version = ?2")?;
    let rows = stmt.query_map(params![model_name, model_version], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Chunk ids that don't yet have an embedding for the given model
/// (drives `embed_pending`'s incremental behaviour).
pub fn list_pending_chunk_ids(conn: &Connection, model_name: &str, model_version: &str) -> Result<Vec<ChunkId>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM chunks c
         LEFT JOIN embeddings e
           ON e.chunk_id = c.id AND e.model_name = ?1 AND e.model_version = ?2
         WHERE e.id IS NULL",
    )?;
    let rows = stmt.query_map(params![model_name, model_version], |row| {
        let id: String = row.get(0)?;
        Ok(id)
    })?;
    Ok(rows
        .filter_map(|r| r.ok())
        .filter_map(|s| s.parse::<uuid::Uuid>().ok())
        .map(ChunkId::from_uuid)
        .collect())
}

pub fn count(conn: &Connection, model_name: &str, model_version: &str) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM embeddings WHERE model_name = ?1 AND model_version = ?2",
        params![model_name, model_version],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

pub fn count_pending(conn: &Connection, model_name: &str, model_version: &str) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks c
         LEFT JOIN embeddings e
           ON e.chunk_id = c.id AND e.model_name = ?1 AND e.model_version = ?2
         WHERE e.id IS NULL",
        params![model_name, model_version],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

/// Delete every embedding for a given model (e.g. before rebuilding
/// after a model upgrade).
pub fn clear(conn: &Connection, model_name: &str, model_version: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM embeddings WHERE model_name = ?1 AND model_version = ?2",
        params![model_name, model_version],
    )?;
    Ok(())
}
