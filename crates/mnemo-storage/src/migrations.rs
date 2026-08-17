//! SQLite schema for Mnemo (plan.md section 5.1 "Storage Layer" and
//! section 64 "Core Data Model").
//!
//! Migrations here are intentionally a single idempotent schema
//! script (`CREATE TABLE IF NOT EXISTS`) rather than a numbered
//! migration chain — that upgrade path can be introduced once the
//! schema needs to evolve across released versions.

use rusqlite::Connection;

use crate::error::{Result, StorageError};

const SCHEMA_SQL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Sources: provenance for every piece of ingested knowledge.
CREATE TABLE IF NOT EXISTS sources (
    id           TEXT PRIMARY KEY,
    source_type  TEXT NOT NULL,
    name         TEXT NOT NULL,
    uri          TEXT,
    reliability  REAL NOT NULL DEFAULT 1.0,
    sensitivity  TEXT NOT NULL DEFAULT 'PRIVATE',
    content_hash TEXT,
    created_at   TEXT,
    indexed_at   TEXT NOT NULL
);

-- Documents: canonical parsed documents belonging to a source.
CREATE TABLE IF NOT EXISTS documents (
    id                TEXT PRIMARY KEY,
    source_id         TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    title             TEXT,
    mime_type         TEXT NOT NULL,
    created_at        TEXT,
    modified_at       TEXT,
    indexed_at        TEXT NOT NULL,
    content_hash      TEXT NOT NULL,
    parser_version    TEXT NOT NULL,
    embedding_version TEXT
);
CREATE INDEX IF NOT EXISTS idx_documents_source ON documents(source_id);
CREATE INDEX IF NOT EXISTS idx_documents_hash ON documents(content_hash);

-- Chunks: retrievable text spans belonging to a document.
CREATE TABLE IF NOT EXISTS chunks (
    id            TEXT PRIMARY KEY,
    document_id   TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    start_offset  INTEGER NOT NULL DEFAULT 0,
    end_offset    INTEGER NOT NULL DEFAULT 0,
    page          INTEGER,
    section       TEXT,
    chunk_index   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);

-- Full-text index over chunk text (plan.md section 7 "Full-Text Search").
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

-- Conversations and messages (plan.md section 18 "Conversation Storage").
CREATE TABLE IF NOT EXISTS conversations (
    id         TEXT PRIMARY KEY,
    title      TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    timestamp       TEXT NOT NULL,
    tool_calls      TEXT,
    tool_results    TEXT,
    metadata        TEXT
);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    content='messages',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- User profile (plan.md section 20 "User Profile").
CREATE TABLE IF NOT EXISTS profile_entries (
    id         TEXT PRIMARY KEY,
    key        TEXT NOT NULL UNIQUE,
    value      TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Memories (plan.md sections 23-26 "Memory Model" / "Lifecycle").
CREATE TABLE IF NOT EXISTS memories (
    id             TEXT PRIMARY KEY,
    content        TEXT NOT NULL,
    memory_type    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'CANDIDATE',
    confidence     REAL NOT NULL DEFAULT 1.0,
    importance     REAL NOT NULL DEFAULT 0.5,
    created_at     TEXT NOT NULL,
    last_accessed  TEXT NOT NULL,
    valid_from     TEXT,
    valid_until    TEXT,
    source_id      TEXT REFERENCES sources(id) ON DELETE SET NULL,
    superseded_by  TEXT REFERENCES memories(id) ON DELETE SET NULL,
    -- When `status` last changed (v4). Gates the SUPERSEDED/EXPIRED
    -- -> ARCHIVED transition behind a grace period instead of
    -- archiving immediately (plan.md section 25 "Memory Lifecycle").
    -- Backfilled from `created_at` for rows that predate this column
    -- — see `ensure_column` below.
    status_changed_at TEXT,
    -- Decay-maintenance anchor (v5, plan.md section 26 "Memory
    -- Importance"): advanced to "now" every time importance decay
    -- runs for this memory, so re-running decay doesn't re-decay the
    -- same elapsed interval, and a genuine access (which bumps
    -- `last_accessed` past this) resets the decay clock. Backfilled
    -- from `created_at` for rows that predate this column.
    last_decay_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
-- idx_memories_status_changed_at is created in `apply()` in Rust,
-- after `ensure_column` guarantees the column exists on databases
-- created before v4 — see below.

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    content='memories',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Embeddings: vector embeddings of chunk text (plan.md section 47
-- "Local Embedding Models" / section 49 "Model Versioning" / Phase 4).
-- `vector` is stored as a JSON array of f32 rather than a BLOB so the
-- schema stays dependency-free (no custom SQLite functions needed to
-- read it back); a real ANN index can replace brute-force scans over
-- this table later without changing this table's shape.
-- The unique constraint lets `count_pending`/`embed_pending` treat
-- "chunk already embedded with the current model" as a single lookup,
-- and re-embedding (e.g. after a model upgrade) as a plain upsert.
CREATE TABLE IF NOT EXISTS embeddings (
    id            TEXT PRIMARY KEY,
    chunk_id      TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
    model_name    TEXT NOT NULL,
    model_version TEXT NOT NULL,
    dimension     INTEGER NOT NULL,
    vector        TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    UNIQUE(chunk_id, model_name, model_version)
);
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk ON embeddings(chunk_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model_name, model_version);

-- Message embeddings: vector embeddings of conversation message
-- content (Phase 8 follow-up — see ROADMAP.md). Mirrors `embeddings`
-- exactly (same JSON-vector rationale, same uniqueness shape) but
-- keyed on `message_id` instead of `chunk_id`, as a separate table
-- rather than a shared/polymorphic one so neither table's foreign key
-- or uniqueness constraint has to become conditional.
CREATE TABLE IF NOT EXISTS message_embeddings (
    id            TEXT PRIMARY KEY,
    message_id    TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    model_name    TEXT NOT NULL,
    model_version TEXT NOT NULL,
    dimension     INTEGER NOT NULL,
    vector        TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    UNIQUE(message_id, model_name, model_version)
);
CREATE INDEX IF NOT EXISTS idx_message_embeddings_message ON message_embeddings(message_id);
CREATE INDEX IF NOT EXISTS idx_message_embeddings_model ON message_embeddings(model_name, model_version);
"#;

/// Current schema version. Bump this and extend `apply` with a
/// numbered migration step when the schema needs to change in a way
/// that isn't safely idempotent (e.g. column removals/renames).
///
/// v2 added the `embeddings` table (Phase 4). v3 added the
/// `message_embeddings` table (Phase 8 follow-up). v4 added the
/// `memories.status_changed_at` column and v5 added
/// `memories.last_decay_at` (both Phase 10 follow-up — memory
/// lifecycle maintenance's archival step needs to know how long a
/// memory has sat in `SUPERSEDED`/`EXPIRED`, and decay needs an
/// anchor independent of `last_accessed`). The first three are
/// additive `CREATE TABLE IF NOT EXISTS`s; v4/v5 are column additions
/// to an existing table, which SQLite's `CREATE TABLE IF NOT EXISTS`
/// cannot retrofit on its own — see `ensure_column` below for the
/// explicit `ALTER TABLE`/backfill these bumps correspond to.
pub const SCHEMA_VERSION: i64 = 5;

/// Apply the full schema to `conn`. Safe to call on every startup.
pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|e| StorageError::Migration(e.to_string()))?;
    ensure_column(conn, "memories", "status_changed_at", "created_at")?;
    ensure_column(conn, "memories", "last_decay_at", "created_at")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_memories_status_changed_at ON memories(status_changed_at)",
        [],
    )
    .map_err(|e| StorageError::Migration(e.to_string()))?;
    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![SCHEMA_VERSION.to_string()],
    )
    .map_err(|e| StorageError::Migration(e.to_string()))?;
    Ok(())
}

/// Add `TEXT` column `column` to `table` and backfill it from
/// `backfill_from` (another column on the same table) for any
/// database created before the column existed. `SCHEMA_SQL`'s
/// `CREATE TABLE IF NOT EXISTS` already declares every current
/// column for brand-new databases, so this is a no-op on those — it
/// only does real work on a database that already has `table` but
/// predates `column`. SQLite has no `ADD COLUMN IF NOT EXISTS`, so
/// column existence is checked via `PRAGMA table_info` first.
///
/// `table`/`column`/`backfill_from` are always trusted, hard-coded
/// call-site literals (see `apply` above) — never user input — so
/// interpolating them into the `ALTER TABLE`/`UPDATE` statements
/// below is safe; `rusqlite` has no bind-parameter syntax for
/// identifiers, only values.
fn ensure_column(conn: &Connection, table: &str, column: &str, backfill_from: &str) -> Result<()> {
    let has_column = conn
        .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = '{column}'"))
        .map_err(|e| StorageError::Migration(e.to_string()))?
        .exists([])
        .map_err(|e| StorageError::Migration(e.to_string()))?;

    if !has_column {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        conn.execute(
            &format!("UPDATE {table} SET {column} = {backfill_from} WHERE {column} IS NULL"),
            [],
        )
        .map_err(|e| StorageError::Migration(e.to_string()))?;
    }

    Ok(())
}
