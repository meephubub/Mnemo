//! Message embedding model (plan.md section 49 "Model Versioning";
//! Phase 8 follow-up — see ROADMAP.md).
//!
//! Mirrors [`super::Embedding`] exactly (same fields, same
//! `(subject_id, model_name, model_version)` uniqueness shape) but
//! keys off a [`MessageId`] instead of a [`ChunkId`], so conversation
//! messages can be embedded and retrieved the same way document
//! chunks are, without disturbing `Embedding`/`embeddings` (which
//! plenty of existing code — `EmbedHandle`, `vector_search`, tests —
//! already assumes is chunk-only). Kept as a separate type rather
//! than generalizing `Embedding` over an enum/union subject id, to
//! avoid a breaking change to that existing surface.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{EmbeddingId, MessageId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageEmbedding {
    pub id: EmbeddingId,
    pub message_id: MessageId,
    pub model_name: String,
    pub model_version: String,
    pub dimension: usize,
    pub vector: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

impl MessageEmbedding {
    pub fn new(
        message_id: MessageId,
        model_name: impl Into<String>,
        model_version: impl Into<String>,
        vector: Vec<f32>,
    ) -> Self {
        let dimension = vector.len();
        Self {
            id: EmbeddingId::new(),
            message_id,
            model_name: model_name.into(),
            model_version: model_version.into(),
            dimension,
            vector,
            created_at: Utc::now(),
        }
    }
}
