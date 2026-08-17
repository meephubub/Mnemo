//! Memory API surface (plan.md sections 23-26, 29).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use mnemo_core::ids::MemoryId;
use mnemo_core::math::cosine_similarity;
use mnemo_core::models::{Memory, MemoryDecision, MemoryStatus, MemoryType};
use mnemo_core::{MnemoError, Result};
use mnemo_embeddings::Embedder;
use mnemo_storage::{repositories::memories as repo, Db};

use crate::blocking;

/// Tunables for [`MemoryStore::run_lifecycle_maintenance`], grouped
/// so a caller can run the whole plan.md section 25/26 policy with
/// one call instead of remembering to invoke `promote_ready`,
/// `expire_temporary`, `decay_importance`, and `archive_stale`
/// separately (and in the right order).
#[derive(Debug, Clone, Copy)]
pub struct LifecyclePolicy {
    /// Passed to [`MemoryStore::promote_ready`]: `Candidate`s at or
    /// above this importance are promoted to `Active`.
    pub promote_min_importance: f32,
    /// Passed to [`MemoryStore::decay_importance`]: importance halves
    /// for every `decay_half_life` that elapses without the memory
    /// being accessed.
    pub decay_half_life: Duration,
    /// Passed to [`MemoryStore::decay_importance`]: importance never
    /// decays below this value — a memory can still be relevant even
    /// if unused for a long time, and a hard floor keeps it
    /// discoverable rather than sorting to the very bottom forever.
    pub decay_floor: f32,
    /// Passed to [`MemoryStore::archive_stale`]: `Superseded`/
    /// `Expired` memories are archived once they've held that status
    /// for at least this long.
    pub archive_grace_period: Duration,
}

impl Default for LifecyclePolicy {
    fn default() -> Self {
        Self {
            promote_min_importance: 0.5,
            decay_half_life: Duration::days(30),
            decay_floor: 0.05,
            archive_grace_period: Duration::days(30),
        }
    }
}

/// Outcome of [`MemoryStore::run_lifecycle_maintenance`] — one count
/// per policy step, in the order they ran.
#[derive(Debug, Clone, Default)]
pub struct LifecycleReport {
    pub promoted: Vec<MemoryId>,
    pub expired: Vec<MemoryId>,
    pub decayed: usize,
    pub archived: Vec<MemoryId>,
}

/// A memory found to conflict with newly proposed content (plan.md
/// section 29 "Contradiction Detection"): same [`MemoryType`], high
/// semantic similarity to the new content, but different text.
#[derive(Debug, Clone)]
pub struct ContradictionMatch {
    pub existing: Memory,
    /// Cosine similarity in `[-1.0, 1.0]` between the new content and
    /// `existing.content`, both embedded with the caller's [`Embedder`].
    pub similarity: f32,
}

/// How [`MemoryStore::propose_with_contradiction_check`] resolved a
/// detected contradiction.
#[derive(Debug, Clone)]
pub enum ContradictionResolution {
    /// The new memory's confidence didn't fall too far below the
    /// existing one's, so the existing memory was marked
    /// [`MemoryStatus::Superseded`] by the new one (plan.md section
    /// 29's worked example: "I use Python" -> "I've switched to
    /// Rust").
    Superseded { old: MemoryId, new: MemoryId, similarity: f32 },
    /// The new memory's confidence was too far below the existing
    /// one's to safely treat it as an update, so the existing memory
    /// was left `Active` and the conflict is surfaced for manual
    /// review instead of being resolved automatically.
    Unresolved { existing: MemoryId, similarity: f32 },
}

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

    /// Move `Superseded`/`Expired` memories to the terminal `Archived`
    /// state once they've held that status for at least
    /// `grace_period` (plan.md section 25's final `-> ARCHIVED` step).
    /// The grace period exists so a just-superseded memory is still
    /// easy to find for a while rather than immediately dropping out
    /// of the "interesting" statuses. Historical evidence is kept —
    /// this only ever flips `status`, never deletes the row. Returns
    /// the ids that were archived.
    pub async fn archive_stale(&self, now: DateTime<Utc>, grace_period: Duration) -> Result<Vec<MemoryId>> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let cutoff = now - grace_period;
            let stale = repo::list_archivable(&conn, cutoff)?;
            let mut ids = Vec::new();
            for memory in stale {
                repo::set_status(&conn, memory.id, MemoryStatus::Archived)?;
                ids.push(memory.id);
            }
            Ok(ids)
        })
        .await
    }

    /// Apply exponential importance decay to every `Active` memory
    /// that hasn't been accessed or decayed since `now - half_life`
    /// (plan.md section 26: importance "influences retrieval, context
    /// packing, memory retention, profile promotion" — a memory that
    /// hasn't been touched in a long time should matter less than one
    /// used yesterday).
    ///
    /// Decay is anchored to `max(last_accessed, last_decay_at)`, not
    /// `last_accessed` alone: every call advances `last_decay_at` to
    /// `now` for whatever it touches, so re-running this repeatedly
    /// with an unchanged `last_accessed` decays only the *newly*
    /// elapsed interval each time rather than re-applying the same
    /// decay from scratch, while a genuine access in between (which
    /// bumps `last_accessed` past the last decay run) resets the
    /// clock for that memory. Importance never drops below `floor`.
    /// Returns the number of memories whose importance changed.
    pub async fn decay_importance(&self, now: DateTime<Utc>, half_life: Duration, floor: f32) -> Result<usize> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            let half_life_secs = half_life.num_seconds().max(1) as f64;
            let mut decayed = 0usize;
            for memory in repo::list_active(&conn)? {
                let anchor = memory.last_accessed.max(memory.last_decay_at);
                let elapsed_secs = (now - anchor).num_seconds();
                if elapsed_secs <= 0 || memory.importance <= floor {
                    continue;
                }
                let decay_factor = 0.5_f64.powf(elapsed_secs as f64 / half_life_secs);
                let new_importance = ((memory.importance as f64) * decay_factor).max(floor as f64) as f32;
                repo::set_importance_and_decay_anchor(&conn, memory.id, new_importance, now)?;
                decayed += 1;
            }
            Ok(decayed)
        })
        .await
    }

    /// Run the full plan.md section 25/26 maintenance policy in the
    /// order that makes sense to apply it: promote candidates that
    /// have cleared the importance bar, expire temporary memories
    /// past their `valid_until`, decay unused `Active` memories'
    /// importance, then archive anything that's been
    /// `Superseded`/`Expired` past its grace period. Intended to be
    /// called periodically (e.g. once per session or on a scheduler)
    /// rather than after every single memory write.
    pub async fn run_lifecycle_maintenance(&self, now: DateTime<Utc>, policy: LifecyclePolicy) -> Result<LifecycleReport> {
        let promoted = self.promote_ready(policy.promote_min_importance).await?;
        let expired = self.expire_temporary(now).await?;
        let decayed = self.decay_importance(now, policy.decay_half_life, policy.decay_floor).await?;
        let archived = self.archive_stale(now, policy.archive_grace_period).await?;
        Ok(LifecycleReport { promoted, expired, decayed, archived })
    }

    /// Search `Active` memories of the same [`MemoryType`] as `content`
    /// for potential contradictions (plan.md section 29): embed
    /// `content` and every candidate with `embedder`, and return every
    /// existing memory whose cosine similarity to `content` is at
    /// least `similarity_threshold`, most similar first. An exact text
    /// match is never returned — identical content is a duplicate, not
    /// a contradiction.
    ///
    /// This embeds candidates on the fly rather than requiring a
    /// dedicated memory-embeddings table: the `Active` memory set for
    /// a single local knowledge base is small relative to documents/
    /// messages, and this is a maintenance-style operation, not a
    /// hot retrieval path.
    pub async fn find_similar_active(
        &self,
        embedder: Arc<dyn Embedder>,
        memory_type: MemoryType,
        content: impl Into<String> + Send + 'static,
        similarity_threshold: f32,
    ) -> Result<Vec<ContradictionMatch>> {
        let db = self.db.clone();
        let content = content.into();
        blocking::run(move || {
            let conn = db.conn();
            let query_vector = embedder.embed(&content).map_err(|e| MnemoError::Embedding(e.to_string()))?;

            let mut matches = Vec::new();
            for existing in repo::list(&conn, Some(MemoryStatus::Active))? {
                if existing.memory_type != memory_type || existing.content == content {
                    continue;
                }
                let existing_vector = embedder.embed(&existing.content).map_err(|e| MnemoError::Embedding(e.to_string()))?;
                let similarity = cosine_similarity(&query_vector, &existing_vector) as f32;
                if similarity >= similarity_threshold {
                    matches.push(ContradictionMatch { existing, similarity });
                }
            }
            matches.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
            Ok(matches)
        })
        .await
    }

    /// [`Self::propose`] a new memory, then check it against existing
    /// `Active` memories of the same type for contradictions
    /// ([`Self::find_similar_active`]) and resolve any found (plan.md
    /// section 29: "Compare timestamps/confidence -> mark previous
    /// memory as superseded").
    ///
    /// Resolution rule: a contradiction is resolved by superseding the
    /// existing memory whenever the new memory's confidence is not
    /// more than `confidence_tolerance` below the existing one's — new
    /// evidence is timestamped later by construction (it's being
    /// proposed right now), so it wins ties and mild confidence drops,
    /// matching the worked example in plan.md ("I use Python" ->
    /// "I've switched to Rust"). If the new memory's confidence is
    /// more than `confidence_tolerance` lower, the existing memory is
    /// left `Active` and the conflict is reported as `Unresolved`
    /// rather than silently overwriting higher-confidence information
    /// with lower-confidence information.
    ///
    /// Contradictions are only checked (and can only be resolved)
    /// against memories that were actually persisted — nothing runs if
    /// [`Self::propose`] rejected the content outright.
    pub async fn propose_with_contradiction_check(
        &self,
        embedder: Arc<dyn Embedder>,
        memory_type: MemoryType,
        content: impl Into<String> + Send + 'static,
        confidence: f32,
        similarity_threshold: f32,
        confidence_tolerance: f32,
    ) -> Result<(MemoryProposal, Vec<ContradictionResolution>)> {
        let content = content.into();

        // Detect against the *pre-existing* Active set, before the
        // new memory (if any) is persisted, so it never matches
        // against itself.
        let candidates = self
            .find_similar_active(embedder.clone(), memory_type, content.clone(), similarity_threshold)
            .await?;

        let proposal = self.propose(memory_type, content, confidence).await?;

        let new_id = match &proposal {
            MemoryProposal::Saved(m) | MemoryProposal::Candidate(m) => Some(m.id),
            MemoryProposal::Rejected => None,
        };

        let mut resolutions = Vec::new();
        if let Some(new_id) = new_id {
            for candidate in candidates {
                if confidence >= candidate.existing.confidence - confidence_tolerance {
                    self.supersede(candidate.existing.id, new_id).await?;
                    resolutions.push(ContradictionResolution::Superseded {
                        old: candidate.existing.id,
                        new: new_id,
                        similarity: candidate.similarity,
                    });
                } else {
                    resolutions.push(ContradictionResolution::Unresolved {
                        existing: candidate.existing.id,
                        similarity: candidate.similarity,
                    });
                }
            }
        }

        Ok((proposal, resolutions))
    }
}
