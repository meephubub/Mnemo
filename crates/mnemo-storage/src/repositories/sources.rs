use mnemo_core::ids::SourceId;
use mnemo_core::models::Source;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::*;

fn from_row(row: &Row) -> rusqlite::Result<Source> {
    let id: String = row.get("id")?;
    let source_type: String = row.get("source_type")?;
    let sensitivity: String = row.get("sensitivity")?;
    let created_at: Option<String> = row.get("created_at")?;
    let indexed_at: String = row.get("indexed_at")?;

    Ok(Source {
        id: id.parse::<uuid::Uuid>().map(SourceId::from_uuid).unwrap_or_default(),
        source_type: str_to_source_type(&source_type).unwrap_or(mnemo_core::models::SourceType::Inference),
        name: row.get("name")?,
        uri: row.get("uri")?,
        reliability: row.get("reliability")?,
        sensitivity: str_to_sensitivity(&sensitivity).unwrap_or_default(),
        content_hash: row.get("content_hash")?,
        created_at: created_at.and_then(|s| str_to_dt(&s).ok()),
        indexed_at: str_to_dt(&indexed_at).unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub fn insert(conn: &Connection, source: &Source) -> Result<()> {
    conn.execute(
        "INSERT INTO sources (id, source_type, name, uri, reliability, sensitivity, content_hash, created_at, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            source.id.to_string(),
            source_type_to_str(source.source_type),
            source.name,
            source.uri,
            source.reliability,
            sensitivity_to_str(source.sensitivity),
            source.content_hash,
            opt_dt_to_str(source.created_at),
            dt_to_str(source.indexed_at),
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: SourceId) -> Result<Source> {
    conn.query_row(
        "SELECT * FROM sources WHERE id = ?1",
        params![id.to_string()],
        from_row,
    )
    .optional()?
    .ok_or_else(|| StorageError::NotFound(format!("source {id}")))
}

pub fn list(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare("SELECT * FROM sources ORDER BY indexed_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn find_by_content_hash(conn: &Connection, hash: &str) -> Result<Option<Source>> {
    Ok(conn
        .query_row(
            "SELECT * FROM sources WHERE content_hash = ?1",
            params![hash],
            from_row,
        )
        .optional()?)
}

pub fn delete(conn: &Connection, id: SourceId) -> Result<()> {
    conn.execute("DELETE FROM sources WHERE id = ?1", params![id.to_string()])?;
    Ok(())
}
