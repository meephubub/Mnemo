//! `mnemo` — the public, async facade over the Mnemo workspace
//! (plan.md section 62 "Core API").
//!
//! ```no_run
//! # async fn example() -> mnemo_core::Result<()> {
//! let db = mnemo::Mnemo::open("mnemo.db")?;
//!
//! db.ingest().ingest_file("notes.md").await?;
//! let hits = db.search().search("project deadline").await?;
//! for hit in hits {
//!     println!("{}", hit.text);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Each `Mnemo::*()` accessor (`profile`, `memories`, `ingest`,
//! `conversations`, `search`) returns a small, `Clone`-able handle
//! that shares the same underlying [`mnemo_storage::Db`] connection,
//! so callers can freely pass handles around instead of the whole
//! `Mnemo` value.

mod blocking;
mod conversation;
mod ingest;
mod memory;
mod profile;
mod search;

pub use conversation::ConversationStore;
pub use ingest::{IngestHandle, IngestOutcome};
pub use memory::MemoryStore;
pub use profile::ProfileHandle;
pub use search::{HitKind, SearchHandle, SearchHit, SearchOptions, SearchScope};

pub use mnemo_core::{ids, models, MnemoError, Result};
pub use mnemo_ingest::parsers::FileKind;

use std::path::Path;

use mnemo_storage::Db;

/// A single open Mnemo database.
///
/// Cheap to clone — it just wraps a shared, mutex-guarded SQLite
/// connection (see [`mnemo_storage::Db`]) — so it can be stored
/// directly in application state (e.g. an `Arc<Mnemo>` is unnecessary;
/// `Mnemo` itself is already shareable).
#[derive(Clone)]
pub struct Mnemo {
    db: Db,
}

impl Mnemo {
    /// Open (creating if necessary) a Mnemo database at `path`,
    /// applying the schema described in plan.md section 64.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self { db: Db::open(path)? })
    }

    /// Open a private in-memory database. Useful for tests and quick
    /// experiments; nothing is persisted.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self { db: Db::open_in_memory()? })
    }

    /// Read/update the small, stable user profile (plan.md section 20).
    pub fn profile(&self) -> ProfileHandle {
        ProfileHandle::new(self.db.clone())
    }

    /// Create, list, and manage durable memories (plan.md sections 23-26).
    pub fn memories(&self) -> MemoryStore {
        MemoryStore::new(self.db.clone())
    }

    /// Ingest files/text into the searchable knowledge base (plan.md section 14).
    pub fn ingest(&self) -> IngestHandle {
        IngestHandle::new(self.db.clone())
    }

    /// Record and browse conversation history (plan.md section 18).
    pub fn conversations(&self) -> ConversationStore {
        ConversationStore::new(self.db.clone())
    }

    /// Query everything Mnemo has indexed (plan.md section 7 / Phase 3).
    pub fn search(&self) -> SearchHandle {
        SearchHandle::new(self.db.clone())
    }
}
