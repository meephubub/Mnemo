//! Document model (plan.md section 12 "Document Model").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DocumentId, SourceId};

/// A canonical, parsed document ready for chunking/indexing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub source_id: SourceId,
    pub title: Option<String>,
    pub mime_type: String,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub indexed_at: DateTime<Utc>,
    pub content_hash: String,
    pub parser_version: String,
    pub embedding_version: Option<String>,
}

impl Document {
    pub fn new(
        source_id: SourceId,
        mime_type: impl Into<String>,
        content_hash: impl Into<String>,
        parser_version: impl Into<String>,
    ) -> Self {
        Self {
            id: DocumentId::new(),
            source_id,
            title: None,
            mime_type: mime_type.into(),
            created_at: None,
            modified_at: None,
            indexed_at: Utc::now(),
            content_hash: content_hash.into(),
            parser_version: parser_version.into(),
            embedding_version: None,
        }
    }
}
