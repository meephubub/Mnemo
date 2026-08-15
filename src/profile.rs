//! Profile API surface (plan.md sections 20-22).

use mnemo_core::models::{Memory, MemoryDecision, ProfileEntry};
use mnemo_core::Result;
use mnemo_storage::{repositories::profile as repo, Db};

use crate::blocking;

/// Outcome of [`ProfileHandle::propose`] (plan.md section 22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileProposal {
    /// Confidence >= 0.85: the key was written immediately.
    Saved,
    /// Confidence 0.50-0.84: not written to the profile. Route this
    /// through [`crate::memory::MemoryStore::propose`] as a
    /// `Candidate` memory instead and promote it into the profile
    /// once confirmed.
    NeedsConfirmation,
    /// Confidence < 0.50: discarded outright.
    Rejected,
}

/// Handle for reading and updating the small, stable user profile.
///
/// Obtained via [`crate::Mnemo::profile`]; cheap to create (it just
/// holds a clone of the shared DB handle).
#[derive(Clone)]
pub struct ProfileHandle {
    db: Db,
}

impl ProfileHandle {
    pub(crate) fn new(db: Db) -> Self {
        Self { db }
    }

    /// Set (or update) a profile key. Confidence follows the policy
    /// in plan.md section 22: callers proposing a low-confidence
    /// update should generally not call this directly, and should
    /// instead route it through memory candidates first.
    pub async fn set(&self, key: impl Into<String> + Send + 'static, value: impl Into<String> + Send + 'static, confidence: f32) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            let conn = db.conn();
            repo::set(&conn, &key.into(), &value.into(), confidence)?;
            Ok(())
        })
        .await
    }

    pub async fn get(&self, key: impl Into<String> + Send + 'static) -> Result<Option<ProfileEntry>> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::get(&db.conn(), &key.into())?)).await
    }

    pub async fn get_all(&self) -> Result<Vec<ProfileEntry>> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::list(&db.conn())?)).await
    }

    pub async fn remove(&self, key: impl Into<String> + Send + 'static) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::remove(&db.conn(), &key.into())?;
            Ok(())
        })
        .await
    }

    /// Privileged operation (plan.md section 71 "Security
    /// Boundaries") — wipes every profile entry. Callers should gate
    /// this behind explicit user approval.
    pub async fn clear(&self) -> Result<()> {
        let db = self.db.clone();
        blocking::run(move || {
            repo::clear(&db.conn())?;
            Ok(())
        })
        .await
    }

    /// Apply the plan.md section 22 confidence policy to a proposed
    /// profile update instead of writing it unconditionally. `>=
    /// 0.85` writes the key immediately; `0.50-0.84` and `< 0.50` are
    /// left for the caller to route through
    /// [`crate::memory::MemoryStore::propose`] (as a review candidate)
    /// or discard, respectively — nothing is written to the profile
    /// in either of those cases.
    pub async fn propose(
        &self,
        key: impl Into<String> + Send + 'static,
        value: impl Into<String> + Send + 'static,
        confidence: f32,
    ) -> Result<ProfileProposal> {
        match Memory::decide(confidence) {
            MemoryDecision::AutoSave => {
                self.set(key, value, confidence).await?;
                Ok(ProfileProposal::Saved)
            }
            MemoryDecision::Candidate => Ok(ProfileProposal::NeedsConfirmation),
            MemoryDecision::Reject => Ok(ProfileProposal::Rejected),
        }
    }
}
