//! Memory API surface (plan.md sections 23-26).

use mnemo_core::ids::MemoryId;
use mnemo_core::models::{Memory, MemoryStatus, MemoryType};
use mnemo_core::Result;
use mnemo_storage::{repositories::memories as repo, Db};

use crate::blocking;

/// Handle for creating, listing, and managing durable memories
/// (facts, preferences, decisions, ...) distinct from raw ingested
/// content.
///
/// Obtained via [`crate::Mnemo::memories`]; cheap to create (it just
/// holds a clone of the shared DB handle).
#[derive(Clone)]
pub struct MemoryStore {
    db: Db,
}

impl MemoryStore {
    pub(crate) fn new(db: Db) -> Self {
        Self { db }
    }

    /// Record a new memory of the given type.
    pub async fn add(&self, memory_type: MemoryType, content: impl Into<String> + Send + 'static) -> Result<Memory> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let memory = Memory::new(content.into(), memory_type);
            repo::insert(&conn, &memory)?;
            Ok(memory)
        })
        .await
    }

    /// Fetch a single memory by id.
    pub async fn get(&self, id: MemoryId) -> Result<Memory> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::get(&db.conn(), id)?)).await
    }

    /// List memories, optionally filtered by lifecycle status,
    /// ordered by importance then recency.
    pub async fn list(&self, status: Option<MemoryStatus>) -> Result<Vec<Memory>> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::list(&db.conn(), status)?)).await
    }

    /// Update the content of an existing memory.
    pub async fn update(&self, id: MemoryId, content: impl Into<String> + Send + 'static) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::update_content(&db.conn(), id, &content.into())?;
            Ok(())
        })
        .await
    }

    /// Transition a memory's lifecycle status (e.g. Candidate -> Active).
    pub async fn set_status(&self, id: MemoryId, status: MemoryStatus) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::set_status(&db.conn(), id, status)?;
            Ok(())
        })
        .await
    }

    /// Mark `old_id` as superseded by a newly created memory, per the
    /// contradiction-handling policy in plan.md section 29.
    pub async fn supersede(&self, old_id: MemoryId, new_id: MemoryId) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::supersede(&db.conn(), old_id, new_id)?;
            Ok(())
        })
        .await
    }

    /// Permanently delete a memory.
    pub async fn delete(&self, id: MemoryId) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::delete(&db.conn(), id)?;
            Ok(())
        })
        .await
    }
}
