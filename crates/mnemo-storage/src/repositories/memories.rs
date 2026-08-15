use mnemo_core::ids::{MemoryId, SourceId};
use mnemo_core::models::{Memory, MemoryStatus};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::*;

fn from_row(row: &Row) -> rusqlite::Result<Memory> {
    let id: String = row.get("id")?;
    let memory_type: String = row.get("memory_type")?;
    let status: String = row.get("status")?;
    let created_at: String = row.get("created_at")?;
    let last_accessed: String = row.get("last_accessed")?;
    let valid_from: Option<String> = row.get("valid_from")?;
    let valid_until: Option<String> = row.get("valid_until")?;
    let source_id: Option<String> = row.get("source_id")?;
    let superseded_by: Option<String> = row.get("superseded_by")?;

    Ok(Memory {
        id: id.parse::<uuid::Uuid>().map(MemoryId::from_uuid).unwrap_or_default(),
        content: row.get("content")?,
        memory_type: str_to_memory_type(&memory_type).unwrap_or(mnemo_core::models::MemoryType::Fact),
        status: str_to_memory_status(&status).unwrap_or(MemoryStatus::Candidate),
        confidence: row.get("confidence")?,
        importance: row.get("importance")?,
        created_at: str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now()),
        last_accessed: str_to_dt(&last_accessed).unwrap_or_else(|_| chrono::Utc::now()),
        valid_from: valid_from.and_then(|s| str_to_dt(&s).ok()),
        valid_until: valid_until.and_then(|s| str_to_dt(&s).ok()),
        source_id: source_id.and_then(|s| s.parse::<uuid::Uuid>().ok()).map(SourceId::from_uuid),
        superseded_by: superseded_by
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .map(MemoryId::from_uuid),
    })
}

pub fn insert(conn: &Connection, memory: &Memory) -> Result<()> {
    conn.execute(
        "INSERT INTO memories (id, content, memory_type, status, confidence, importance, created_at, last_accessed, valid_from, valid_until, source_id, superseded_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            memory.id.to_string(),
            memory.content,
            memory_type_to_str(memory.memory_type),
            memory_status_to_str(memory.status),
            memory.confidence,
            memory.importance,
            dt_to_str(memory.created_at),
            dt_to_str(memory.last_accessed),
            opt_dt_to_str(memory.valid_from),
            opt_dt_to_str(memory.valid_until),
            memory.source_id.map(|s| s.to_string()),
            memory.superseded_by.map(|s| s.to_string()),
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: MemoryId) -> Result<Memory> {
    conn.query_row("SELECT * FROM memories WHERE id = ?1", params![id.to_string()], from_row)
        .optional()?
        .ok_or_else(|| StorageError::NotFound(format!("memory {id}")))
}

pub fn list(conn: &Connection, status: Option<MemoryStatus>) -> Result<Vec<Memory>> {
    let mut stmt = if status.is_some() {
        conn.prepare("SELECT * FROM memories WHERE status = ?1 ORDER BY importance DESC, created_at DESC")?
    } else {
        conn.prepare("SELECT * FROM memories ORDER BY importance DESC, created_at DESC")?
    };

    let rows = if let Some(status) = status {
        stmt.query_map(params![memory_status_to_str(status)], from_row)?
    } else {
        stmt.query_map([], from_row)?
    };
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn set_status(conn: &Connection, id: MemoryId, status: MemoryStatus) -> Result<()> {
    conn.execute(
        "UPDATE memories SET status = ?1 WHERE id = ?2",
        params![memory_status_to_str(status), id.to_string()],
    )?;
    Ok(())
}

pub fn supersede(conn: &Connection, old_id: MemoryId, new_id: MemoryId) -> Result<()> {
    conn.execute(
        "UPDATE memories SET status = 'SUPERSEDED', superseded_by = ?1 WHERE id = ?2",
        params![new_id.to_string(), old_id.to_string()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: MemoryId) -> Result<()> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id.to_string()])?;
    Ok(())
}
