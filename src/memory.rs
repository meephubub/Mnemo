//! Memory API surface (plan.md sections 23-26).

use chrono::{DateTime, Utc};
use mnemo_core::ids::MemoryId;
use mnemo_core::models::{Memory, MemoryDecision, MemoryStatus, MemoryType};
use mnemo_core::Result;
use mnemo_storage::{repositories::memories as repo, Db};

use crate::blocking;

/// Outcome of [`MemoryStore::propose`] — mirrors [`MemoryDecision`]
/// but carries the resulting memory (if one was persisted).
#[derive(Debug, Clone)]
pub enum MemoryProposal {
    /// Confidence >= 0.85: persisted immediately as `Active`.
    Saved(Memory),
    /// Confidence 0.50-0.84: persisted as `Candidate` for later
    /// review via [`MemoryStore::promote_ready`].
    Candidate(Memory),
    /// Confidence < 0.50: nothing was persisted.
    Rejected,
}

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

    /// Apply the plan.md section 22 confidence policy to a proposed
    /// memory: `>= 0.85` is persisted immediately as `Active`,
    /// `0.50-0.84` is persisted as `Candidate` (see
    /// [`Self::promote_ready`]), and `< 0.50` is rejected outright
    /// and never touches storage.
    ///
    /// Callers handling sensitive content should tighten `confidence`
    /// (e.g. subtract a penalty) before calling this, per the same
    /// section's note that "sensitive information should have
    /// stricter rules".
    pub async fn propose(
        &self,
        memory_type: MemoryType,
        content: impl Into<String> + Send + 'static,
        confidence: f32,
    ) -> Result<MemoryProposal> {
        let decision = Memory::decide(confidence);
        if decision == MemoryDecision::Reject {
            return Ok(MemoryProposal::Rejected);
        }

        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let mut memory = Memory::new(content.into(), memory_type);
            memory.confidence = confidence;
            memory.status = match decision {
                MemoryDecision::AutoSave => MemoryStatus::Active,
                MemoryDecision::Candidate => MemoryStatus::Candidate,
                MemoryDecision::Reject => unreachable!("rejected proposals never reach storage"),
            };
            repo::insert(&conn, &memory)?;
            Ok(match decision {
                MemoryDecision::AutoSave => MemoryProposal::Saved(memory),
                MemoryDecision::Candidate => MemoryProposal::Candidate(memory),
                MemoryDecision::Reject => unreachable!(),
            })
        })
        .await
    }

    /// Promote `Candidate` memories to `Active` once they clear an
    /// importance bar (plan.md section 26: importance "influences
    /// ... profile promotion"). Candidates below the bar are left
    /// untouched for future re-evaluation. Returns the ids that were
    /// promoted.
    pub async fn promote_ready(&self, min_importance: f32) -> Result<Vec<MemoryId>> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let candidates = repo::list(&conn, Some(MemoryStatus::Candidate))?;
            let mut promoted = Vec::new();
            for memory in candidates {
                if memory.importance >= min_importance {
                    repo::set_status(&conn, memory.id, MemoryStatus::Active)?;
                    promoted.push(memory.id);
                }
            }
            Ok(promoted)
        })
        .await
    }

    /// Transition `Temporary` memories whose `valid_until` has
    /// already passed to `Expired` (plan.md section 25:
    /// `CANDIDATE -> TEMPORARY -> EXPIRE`). Historical evidence is
    /// kept — this only flips `status`, it never deletes the row.
    /// Returns the ids that were expired.
    pub async fn expire_temporary(&self, now: DateTime<Utc>) -> Result<Vec<MemoryId>> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let expired = repo::list_expired_temporary(&conn, now)?;
            let mut ids = Vec::new();
            for memory in expired {
                repo::set_status(&conn, memory.id, MemoryStatus::Expired)?;
                ids.push(memory.id);
            }
            Ok(ids)
        })
        .await
    }

    /// Update a memory's importance score (plan.md section 26).
    pub async fn set_importance(&self, id: MemoryId, importance: f32) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::set_importance(&db.conn(), id, importance)?;
            Ok(())
        })
        .await
    }

    /// Set the validity window for a temporal memory (plan.md
    /// section 11 "Temporal Memory").
    pub async fn set_valid_range(
        &self,
        id: MemoryId,
        valid_from: Option<DateTime<Utc>>,
        valid_until: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::set_valid_range(&db.conn(), id, valid_from, valid_until)?;
            Ok(())
        })
        .await
    }

    /// Record that a memory was just used during retrieval, updating
    /// `last_accessed` for recency-aware ranking (plan.md section 27).
    pub async fn touch(&self, id: MemoryId) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::touch_last_accessed(&db.conn(), id)?;
            Ok(())
        })
        .await
    }
}
