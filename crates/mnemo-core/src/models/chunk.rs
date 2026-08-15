//! Chunk model (plan.md section 13 "Chunking").

use serde::{Deserialize, Serialize};

use crate::ids::{ChunkId, DocumentId};

/// A retrievable unit of text carved out of a [`Document`](super::document::Document).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub page: Option<u32>,
    pub section: Option<String>,
    /// Ordinal position of this chunk within its parent document, used
    /// for stable ordering and neighbour lookups during context packing.
    pub chunk_index: usize,
}

impl Chunk {
    pub fn new(document_id: DocumentId, text: impl Into<String>, chunk_index: usize) -> Self {
        let text = text.into();
        let end_offset = text.len();
        Self {
            id: ChunkId::new(),
            document_id,
            text,
            start_offset: 0,
            end_offset,
            page: None,
            section: None,
            chunk_index,
        }
    }
}
