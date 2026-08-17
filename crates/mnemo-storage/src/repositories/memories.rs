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
    let status_changed_at: Option<String> = row.get("status_changed_at")?;
    let last_decay_at: Option<String> = row.get("last_decay_at")?;

    let created_at_dt = str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now());

    Ok(Memory {
        id: id.parse::<uuid::Uuid>().map(MemoryId::from_uuid).unwrap_or_default(),
        content: row.get("content")?,
        memory_type: str_to_memory_type(&memory_type).unwrap_or(mnemo_core::models::MemoryType::Fact),
        status: str_to_memory_status(&status).unwrap_or(MemoryStatus::Candidate),
        confidence: row.get("confidence")?,
        importance: row.get("importance")?,
        created_at: created_at_dt,
        last_accessed: str_to_dt(&last_accessed).unwrap_or_else(|_| chrono::Utc::now()),
        valid_from: valid_from.and_then(|s| str_to_dt(&s).ok()),
        valid_until: valid_until.and_then(|s| str_to_dt(&s).ok()),
        source_id: source_id.and_then(|s| s.parse::<uuid::Uuid>().ok()).map(SourceId::from_uuid),
        superseded_by: superseded_by
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .map(MemoryId::from_uuid),
        // Both fall back to `created_at` for rows written before
        // their column existed (backfilled in
        // `migrations::ensure_column`, but this covers any gap
        // between upgrade and backfill too).
        status_changed_at: status_changed_at.and_then(|s| str_to_dt(&s).ok()).unwrap_or(created_at_dt),
        last_decay_at: last_decay_at.and_then(|s| str_to_dt(&s).ok()).unwrap_or(created_at_dt),
    })
}

pub fn insert(conn: &Connection, memory: &Memory) -> Result<()> {
    conn.execute(
        "INSERT INTO memories (id, content, memory_type, status, confidence, importance, created_at, last_accessed, valid_from, valid_until, source_id, superseded_by, status_changed_at, last_decay_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
            dt_to_str(memory.status_changed_at),
            dt_to_str(memory.last_decay_at),
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

pub fn update_content(conn: &Connection, id: MemoryId, content: &str) -> Result<()> {
    conn.execute(
        "UPDATE memories SET content = ?1 WHERE id = ?2",
        params![content, id.to_string()],
    )?;
    Ok(())
}

/// Transition `id` to `status`, recording the transition time in
/// `status_changed_at` (used by [`list_archivable`] to gate the
/// `SUPERSEDED`/`EXPIRED` -> `ARCHIVED` step behind a grace period —
/// plan.md section 25).
pub fn set_status(conn: &Connection, id: MemoryId, status: MemoryStatus) -> Result<()> {
    conn.execute(
        "UPDATE memories SET status = ?1, status_changed_at = ?2 WHERE id = ?3",
        params![memory_status_to_str(status), dt_to_str(chrono::Utc::now()), id.to_string()],
    )?;
    Ok(())
}

/// Mark `old_id` as `SUPERSEDED` by `new_id` (plan.md section 29
/// "Contradiction Detection"), recording the transition time the
/// same way [`set_status`] does.
pub fn supersede(conn: &Connection, old_id: MemoryId, new_id: MemoryId) -> Result<()> {
    conn.execute(
        "UPDATE memories SET status = 'SUPERSEDED', superseded_by = ?1, status_changed_at = ?2 WHERE id = ?3",
        params![new_id.to_string(), dt_to_str(chrono::Utc::now()), old_id.to_string()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: MemoryId) -> Result<()> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id.to_string()])?;
    Ok(())
}

pub fn set_importance(conn: &Connection, id: MemoryId, importance: f32) -> Result<()> {
    conn.execute(
        "UPDATE memories SET importance = ?1 WHERE id = ?2",
        params![importance, id.to_string()],
    )?;
    Ok(())
}

pub fn set_valid_range(
    conn: &Connection,
    id: MemoryId,
    valid_from: Option<chrono::DateTime<chrono::Utc>>,
    valid_until: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    conn.execute(
        "UPDATE memories SET valid_from = ?1, valid_until = ?2 WHERE id = ?3",
        params![opt_dt_to_str(valid_from), opt_dt_to_str(valid_until), id.to_string()],
    )?;
    Ok(())
}

pub fn touch_last_accessed(conn: &Connection, id: MemoryId) -> Result<()> {
    conn.execute(
        "UPDATE memories SET last_accessed = ?1 WHERE id = ?2",
        params![dt_to_str(chrono::Utc::now()), id.to_string()],
    )?;
    Ok(())
}

/// Temporary memories whose `valid_until` has already passed
/// (plan.md section 25: `CANDIDATE -> TEMPORARY -> EXPIRE`).
pub fn list_expired_temporary(conn: &Connection, now: chrono::DateTime<chrono::Utc>) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM memories WHERE status = 'TEMPORARY' AND valid_until IS NOT NULL AND valid_until <= ?1
         ORDER BY valid_until ASC",
    )?;
    let rows = stmt.query_map(params![dt_to_str(now)], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// `SUPERSEDED`/`EXPIRED` memories that have sat in that status since
/// before `cutoff` — i.e. their grace period has elapsed and they're
/// ready to move to the terminal `ARCHIVED` state (plan.md section 25).
/// Historical evidence is still never deleted; this only ever flips
/// `status`.
pub fn list_archivable(conn: &Connection, cutoff: chrono::DateTime<chrono::Utc>) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM memories WHERE status IN ('SUPERSEDED', 'EXPIRED') AND status_changed_at <= ?1
         ORDER BY status_changed_at ASC",
    )?;
    let rows = stmt.query_map(params![dt_to_str(cutoff)], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// `ACTIVE` memories, for the importance-decay maintenance pass
/// (plan.md section 26). Only `ACTIVE` memories decay — `CANDIDATE`s
/// haven't been reviewed yet, and `TEMPORARY`/`SUPERSEDED`/`EXPIRED`/
/// `ARCHIVED` memories are already past the point where "is this
/// still relevant" applies.
pub fn list_active(conn: &Connection) -> Result<Vec<Memory>> {
    list(conn, Some(MemoryStatus::Active))
}

/// Update `importance` and advance `last_decay_at` to `decayed_at` in
/// one write (plan.md section 26). Keeping both in the same statement
/// means a decay pass can never persist a new importance value
/// without also advancing the anchor that keeps the next pass from
/// re-decaying the same elapsed interval.
pub fn set_importance_and_decay_anchor(
    conn: &Connection,
    id: MemoryId,
    importance: f32,
    decayed_at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    conn.execute(
        "UPDATE memories SET importance = ?1, last_decay_at = ?2 WHERE id = ?3",
        params![importance, dt_to_str(decayed_at), id.to_string()],
    )?;
    Ok(())
}
