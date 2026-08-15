//! Embedding model (plan.md section 49 "Model Versioning").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ChunkId, EmbeddingId};

/// A vector embedding of a chunk's text, produced by a specific
/// embedding model version (plan.md section 47 "Local Embedding
/// Models" / section 49 "Model Versioning").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub id: EmbeddingId,
    pub chunk_id: ChunkId,
    pub model_name: String,
    pub model_version: String,
    pub dimension: usize,
    pub vector: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

impl Embedding {
    pub fn new(
        chunk_id: ChunkId,
        model_name: impl Into<String>,
        model_version: impl Into<String>,
        vector: Vec<f32>,
    ) -> Self {
        let dimension = vector.len();
        Self {
            id: EmbeddingId::new(),
            chunk_id,
            model_name: model_name.into(),
            model_version: model_version.into(),
            dimension,
            vector,
            created_at: Utc::now(),
        }
    }
}
