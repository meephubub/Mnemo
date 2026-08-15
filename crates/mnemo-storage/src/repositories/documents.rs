use mnemo_core::ids::{DocumentId, SourceId};
use mnemo_core::models::Document;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::*;

fn from_row(row: &Row) -> rusqlite::Result<Document> {
    let id: String = row.get("id")?;
    let source_id: String = row.get("source_id")?;
    let created_at: Option<String> = row.get("created_at")?;
    let modified_at: Option<String> = row.get("modified_at")?;
    let indexed_at: String = row.get("indexed_at")?;

    Ok(Document {
        id: id.parse::<uuid::Uuid>().map(DocumentId::from_uuid).unwrap_or_default(),
        source_id: source_id.parse::<uuid::Uuid>().map(SourceId::from_uuid).unwrap_or_default(),
        title: row.get("title")?,
        mime_type: row.get("mime_type")?,
        created_at: created_at.and_then(|s| str_to_dt(&s).ok()),
        modified_at: modified_at.and_then(|s| str_to_dt(&s).ok()),
        indexed_at: str_to_dt(&indexed_at).unwrap_or_else(|_| chrono::Utc::now()),
        content_hash: row.get("content_hash")?,
        parser_version: row.get("parser_version")?,
        embedding_version: row.get("embedding_version")?,
    })
}

pub fn insert(conn: &Connection, doc: &Document) -> Result<()> {
    conn.execute(
        "INSERT INTO documents (id, source_id, title, mime_type, created_at, modified_at, indexed_at, content_hash, parser_version, embedding_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            doc.id.to_string(),
            doc.source_id.to_string(),
            doc.title,
            doc.mime_type,
            opt_dt_to_str(doc.created_at),
            opt_dt_to_str(doc.modified_at),
            dt_to_str(doc.indexed_at),
            doc.content_hash,
            doc.parser_version,
            doc.embedding_version,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: DocumentId) -> Result<Document> {
    conn.query_row(
        "SELECT * FROM documents WHERE id = ?1",
        params![id.to_string()],
        from_row,
    )
    .optional()?
    .ok_or_else(|| StorageError::NotFound(format!("document {id}")))
}

pub fn list(conn: &Connection) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare("SELECT * FROM documents ORDER BY indexed_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn delete(conn: &Connection, id: DocumentId) -> Result<()> {
    conn.execute("DELETE FROM documents WHERE id = ?1", params![id.to_string()])?;
    Ok(())
}
