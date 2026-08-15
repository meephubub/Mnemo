use mnemo_core::models::ProfileEntry;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::Result;
use crate::util::*;

fn from_row(row: &Row) -> rusqlite::Result<ProfileEntry> {
    let id: String = row.get("id")?;
    let created_at: String = row.get("created_at")?;
    let updated_at: String = row.get("updated_at")?;
    Ok(ProfileEntry {
        id: id
            .parse::<uuid::Uuid>()
            .map(mnemo_core::ids::ProfileEntryId::from_uuid)
            .unwrap_or_default(),
        key: row.get("key")?,
        value: row.get("value")?,
        confidence: row.get("confidence")?,
        created_at: str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: str_to_dt(&updated_at).unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Insert or update a profile entry by key (plan.md section 21
/// "Profile Updates" — `update_profile` is idempotent per key).
pub fn set(conn: &Connection, key: &str, value: &str, confidence: f32) -> Result<()> {
    let now = dt_to_str(chrono::Utc::now());
    conn.execute(
        "INSERT INTO profile_entries (id, key, value, confidence, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, confidence = excluded.confidence, updated_at = excluded.updated_at",
        params![
            mnemo_core::ids::ProfileEntryId::new().to_string(),
            key,
            value,
            confidence,
            now,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, key: &str) -> Result<Option<ProfileEntry>> {
    Ok(conn
        .query_row("SELECT * FROM profile_entries WHERE key = ?1", params![key], from_row)
        .optional()?)
}

pub fn list(conn: &Connection) -> Result<Vec<ProfileEntry>> {
    let mut stmt = conn.prepare("SELECT * FROM profile_entries ORDER BY key ASC")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM profile_entries WHERE key = ?1", params![key])?;
    Ok(())
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM profile_entries", [])?;
    Ok(())
}
