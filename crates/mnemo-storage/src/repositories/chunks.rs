use mnemo_core::ids::{ChunkId, DocumentId};
use mnemo_core::models::Chunk;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};

pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Chunk> {
    let id: String = row.get("id")?;
    let document_id: String = row.get("document_id")?;
    let page: Option<i64> = row.get("page")?;

    Ok(Chunk {
        id: id.parse::<uuid::Uuid>().map(ChunkId::from_uuid).unwrap_or_default(),
        document_id: document_id
            .parse::<uuid::Uuid>()
            .map(DocumentId::from_uuid)
            .unwrap_or_default(),
        text: row.get("text")?,
        start_offset: row.get::<_, i64>("start_offset")? as usize,
        end_offset: row.get::<_, i64>("end_offset")? as usize,
        page: page.map(|p| p as u32),
        section: row.get("section")?,
        chunk_index: row.get::<_, i64>("chunk_index")? as usize,
    })
}

pub fn insert(conn: &Connection, chunk: &Chunk) -> Result<()> {
    conn.execute(
        "INSERT INTO chunks (id, document_id, text, start_offset, end_offset, page, section, chunk_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            chunk.id.to_string(),
            chunk.document_id.to_string(),
            chunk.text,
            chunk.start_offset as i64,
            chunk.end_offset as i64,
            chunk.page.map(|p| p as i64),
            chunk.section,
            chunk.chunk_index as i64,
        ],
    )?;
    Ok(())
}

pub fn insert_many(conn: &Connection, chunks: &[Chunk]) -> Result<()> {
    for chunk in chunks {
        insert(conn, chunk)?;
    }
    Ok(())
}

pub fn get(conn: &Connection, id: ChunkId) -> Result<Chunk> {
    conn.query_row("SELECT * FROM chunks WHERE id = ?1", params![id.to_string()], from_row)
        .optional()?
        .ok_or_else(|| StorageError::NotFound(format!("chunk {id}")))
}

pub fn list_for_document(conn: &Connection, document_id: DocumentId) -> Result<Vec<Chunk>> {
    let mut stmt = conn.prepare("SELECT * FROM chunks WHERE document_id = ?1 ORDER BY chunk_index ASC")?;
    let rows = stmt.query_map(params![document_id.to_string()], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn delete_for_document(conn: &Connection, document_id: DocumentId) -> Result<()> {
    conn.execute(
        "DELETE FROM chunks WHERE document_id = ?1",
        params![document_id.to_string()],
    )?;
    Ok(())
}
