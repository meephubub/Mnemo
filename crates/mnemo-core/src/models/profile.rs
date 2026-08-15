//! User profile model (plan.md sections 20-22:
//! "User Profile", "Profile Updates", "Profile Update Rules").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::ProfileEntryId;

/// A single key/value fact in the small, stable user profile.
///
/// Profile entries are intentionally lightweight (a key, a JSON-ish
/// string value, and provenance/confidence) so the whole profile can
/// always be injected into a prompt cheaply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub id: ProfileEntryId,
    pub key: String,
    pub value: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProfileEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>, confidence: f32) -> Self {
        let now = Utc::now();
        Self {
            id: ProfileEntryId::new(),
            key: key.into(),
            value: value.into(),
            confidence,
            created_at: now,
            updated_at: now,
        }
    }
}
