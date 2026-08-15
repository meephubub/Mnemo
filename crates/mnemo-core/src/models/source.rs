//! Source provenance model (plan.md section 35 "Source Provenance",
//! section 37 "Source Reliability", section 72 "Sensitive Information").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::SourceId;

/// Where a piece of knowledge originally came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceType {
    File,
    Email,
    Conversation,
    Webpage,
    Profile,
    Inference,
    UserStatement,
}

impl SourceType {
    /// Default reliability weight for this source type (section 37).
    /// Callers may override this per-source via [`Source::reliability`].
    pub fn default_reliability(&self) -> f32 {
        match self {
            SourceType::UserStatement => 1.0,
            SourceType::File => 1.0,
            SourceType::Email => 0.95,
            SourceType::Conversation => 0.85,
            SourceType::Webpage => 0.75,
            SourceType::Profile => 1.0,
            SourceType::Inference => 0.50,
        }
    }
}

/// Sensitivity level for a source, used to gate automatic profile
/// extraction and cloud/external processing (plan.md section 72).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Sensitivity {
    Public,
    Private,
    Sensitive,
}

impl Default for Sensitivity {
    fn default() -> Self {
        Sensitivity::Private
    }
}

/// A traceable origin for any ingested knowledge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub source_type: SourceType,
    pub name: String,
    pub uri: Option<String>,
    pub reliability: f32,
    pub sensitivity: Sensitivity,
    pub content_hash: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub indexed_at: DateTime<Utc>,
}

impl Source {
    pub fn new(source_type: SourceType, name: impl Into<String>) -> Self {
        let reliability = source_type.default_reliability();
        Self {
            id: SourceId::new(),
            source_type,
            name: name.into(),
            uri: None,
            reliability,
            sensitivity: Sensitivity::default(),
            content_hash: None,
            created_at: None,
            indexed_at: Utc::now(),
        }
    }
}
