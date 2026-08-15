//! Connection management for the SQLite-backed metadata store.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::Result;
use crate::migrations;

/// A thread-safe handle to the Mnemo SQLite database.
///
/// `rusqlite::Connection` is `Send` but not `Sync`; wrapping it in a
/// `Mutex` gives us a single, serialized connection that is cheap to
/// clone (via `Arc`) and safe to share across the async facade in the
/// top-level `mnemo` crate. This is the "single-writer" model SQLite
/// wants anyway, and is sufficient for the local-first, single-process
/// usage described in plan.md — a connection pool can replace this
/// later without changing the repository APIs.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open (creating if necessary) a Mnemo database at `path` and
    /// apply the schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Open a private in-memory database. Useful for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::apply(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Lock and access the underlying connection. Repositories take a
    /// `&Connection`, so callers typically write:
    /// `let conn = db.conn(); repo::get(&conn, id)?;`
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("mnemo db mutex poisoned")
    }
}
