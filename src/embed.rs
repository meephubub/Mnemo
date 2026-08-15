//! Embedding API surface (plan.md section 47 "Local Embedding
//! Models" / section 80 Phase 4; message coverage is a Phase 8
//! follow-up — see ROADMAP.md).
//!
//! Thin async wrapper over `mnemo-storage`'s embeddings repositories:
//! embeds chunks and/or conversation messages that don't yet have a
//! vector for the handle's `Embedder`, and exposes counts/lookup/clear
//! for that model, for each. Swap in a real local model (ONNX/Candle)
//! by implementing [`mnemo_embeddings::Embedder`] and passing it to
//! [`crate::Mnemo::embed_with`] instead of the default
//! [`mnemo_embeddings::HashingEmbedder`].

use std::sync::Arc;

use mnemo_core::ids::{ChunkId, MessageId};
use mnemo_core::models::{Embedding, MessageEmbedding};
use mnemo_core::{MnemoError, Result};
use mnemo_embeddings::Embedder;
use mnemo_storage::repositories::{chunks, conversations, embeddings as embeddings_repo, message_embeddings as message_embeddings_repo};
use mnemo_storage::Db;

use crate::blocking;

/// Handle for generating and inspecting vector embeddings of ingested
/// chunks.
///
/// Obtained via [`crate::Mnemo::embed`] (default hashing embedder) or
/// [`crate::Mnemo::embed_with`] (custom embedder); cheap to create.
#[derive(Clone)]
pub struct EmbedHandle {
    db: Db,
    embedder: Arc<dyn Embedder>,
}

impl EmbedHandle {
    pub(crate) fn new(db: Db, embedder: Arc<dyn Embedder>) -> Self {
        Self { db, embedder }
    }

    /// The embedder backing this handle — pass it to
    /// [`crate::SearchHandle::search_vector`] or
    /// [`crate::SearchHandle::search_hybrid`] so query embedding uses
    /// the same model/version as the stored vectors.
    pub fn embedder(&self) -> Arc<dyn Embedder> {
        self.embedder.clone()
    }

    /// Embed every chunk that doesn't yet have a vector for this
    /// handle's `(model_name, model_version)` (plan.md section 15
    /// "Incremental Indexing": changing the embedding model allows
    /// selective reprocessing since pending is computed per model).
    /// Returns the number of chunks embedded.
    pub async fn embed_pending(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            let conn = db.conn();
            let pending = embeddings_repo::list_pending_chunk_ids(&conn, embedder.model_name(), embedder.model_version())?;
            let mut embedded = 0usize;
            for chunk_id in pending {
                let chunk = chunks::get(&conn, chunk_id)?;
                let vector = embedder
                    .embed(&chunk.text)
                    .map_err(|e| MnemoError::Embedding(e.to_string()))?;
                let embedding = Embedding::new(chunk_id, embedder.model_name(), embedder.model_version(), vector);
                embeddings_repo::upsert(&conn, &embedding)?;
                embedded += 1;
            }
            Ok(embedded)
        })
        .await
    }

    /// Number of chunks already embedded for this handle's model.
    pub async fn count(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || Ok(embeddings_repo::count(&db.conn(), embedder.model_name(), embedder.model_version())?)).await
    }

    /// Number of chunks still lacking an embedding for this handle's
    /// model.
    pub async fn count_pending(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            Ok(embeddings_repo::count_pending(
                &db.conn(),
                embedder.model_name(),
                embedder.model_version(),
            )?)
        })
        .await
    }

    /// Delete every embedding for this handle's model (e.g. before
    /// rebuilding after switching to a different embedder).
    pub async fn clear(&self) -> Result<()> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            embeddings_repo::clear(&db.conn(), embedder.model_name(), embedder.model_version())?;
            Ok(())
        })
        .await
    }

    /// Fetch the stored embedding for a single chunk, if one exists
    /// for this handle's model.
    pub async fn get(&self, chunk_id: ChunkId) -> Result<Embedding> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            Ok(embeddings_repo::get(
                &db.conn(),
                chunk_id,
                embedder.model_name(),
                embedder.model_version(),
            )?)
        })
        .await
    }

    /// Embed every conversation message that doesn't yet have a
    /// vector for this handle's `(model_name, model_version)` — the
    /// same incremental behaviour as [`Self::embed_pending`], applied
    /// to `messages` instead of `chunks`. Returns the number of
    /// messages embedded.
    pub async fn embed_pending_messages(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            let conn = db.conn();
            let pending =
                message_embeddings_repo::list_pending_message_ids(&conn, embedder.model_name(), embedder.model_version())?;
            let mut embedded = 0usize;
            for message_id in pending {
                let message = conversations::get_message(&conn, message_id)?;
                let vector = embedder
                    .embed(&message.content)
                    .map_err(|e| MnemoError::Embedding(e.to_string()))?;
                let embedding = MessageEmbedding::new(message_id, embedder.model_name(), embedder.model_version(), vector);
                message_embeddings_repo::upsert(&conn, &embedding)?;
                embedded += 1;
            }
            Ok(embedded)
        })
        .await
    }

    /// Number of messages already embedded for this handle's model.
    pub async fn count_messages(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || Ok(message_embeddings_repo::count(&db.conn(), embedder.model_name(), embedder.model_version())?))
            .await
    }

    /// Number of messages still lacking an embedding for this
    /// handle's model.
    pub async fn count_pending_messages(&self) -> Result<usize> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            Ok(message_embeddings_repo::count_pending(
                &db.conn(),
                embedder.model_name(),
                embedder.model_version(),
            )?)
        })
        .await
    }

    /// Delete every message embedding for this handle's model (e.g.
    /// before rebuilding after switching to a different embedder).
    pub async fn clear_messages(&self) -> Result<()> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            message_embeddings_repo::clear(&db.conn(), embedder.model_name(), embedder.model_version())?;
            Ok(())
        })
        .await
    }

    /// Fetch the stored embedding for a single message, if one
    /// exists for this handle's model.
    pub async fn get_message(&self, message_id: MessageId) -> Result<MessageEmbedding> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || {
            Ok(message_embeddings_repo::get(
                &db.conn(),
                message_id,
                embedder.model_name(),
                embedder.model_version(),
            )?)
        })
        .await
    }
}
