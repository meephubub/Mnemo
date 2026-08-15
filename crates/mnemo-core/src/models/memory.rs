//! Memory model and lifecycle (plan.md sections 23-26:
//! "Memory Model", "Memory Types", "Memory Lifecycle", "Memory Importance").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{MemoryId, SourceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryType {
    Fact,
    Preference,
    Interest,
    Goal,
    Project,
    Person,
    Location,
    Routine,
    Decision,
    Event,
    Temporary,
}

/// Lifecycle state of a memory (section 25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Temporary,
    Superseded,
    Archived,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub content: String,
    pub memory_type: MemoryType,
    pub status: MemoryStatus,
    pub confidence: f32,
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub source_id: Option<SourceId>,
    /// If this memory superseded/was superseded by another, that link
    /// is recorded here rather than deleting historical evidence
    /// (section 25 / section 29 "Contradiction Detection").
    pub superseded_by: Option<MemoryId>,
}

impl Memory {
    pub fn new(content: impl Into<String>, memory_type: MemoryType) -> Self {
        let now = Utc::now();
        Self {
            id: MemoryId::new(),
            content: content.into(),
            memory_type,
            status: MemoryStatus::Candidate,
            confidence: 1.0,
            importance: 0.5,
            created_at: now,
            last_accessed: now,
            valid_from: None,
            valid_until: None,
            source_id: None,
            superseded_by: None,
        }
    }

    /// Suggested storage policy for a candidate memory based on
    /// confidence (plan.md section 22 "Profile Update Rules" applies
    /// the same thresholds to memories).
    pub fn should_auto_save(confidence: f32) -> bool {
        confidence >= 0.85
    }

    pub fn is_temporary_candidate(confidence: f32) -> bool {
        (0.50..0.85).contains(&confidence)
    }
}
