//! Search API surface (plan.md section 7 "Full-Text Search" / Phase
//! 3, section 6 "Vector Storage" / Phase 4, section 8 "Hybrid
//! Retrieval" / Phase 5).
//!
//! Thin async wrappers over `mnemo-search`'s synchronous lexical,
//! vector, and hybrid search so they fit the same
//! `spawn_blocking`-backed pattern as the rest of the facade.
//! Reranking (Phase 6) is an opt-in second stage over a candidate
//! pool — see [`crate::Reranker`] / [`crate::HeuristicReranker`] and
//! [`crate::ContextRequest::reranker`] — rather than something these
//! plain search functions run on their own. Context packing (Phase 7)
//! lives in [`crate::context`] since it returns a different shape
//! than a ranked hit list.

use std::sync::Arc;

pub use mnemo_search::{HitKind, HybridWeights, SearchHit, SearchOptions, SearchScope};

use mnemo_core::Result;
use mnemo_embeddings::Embedder;
use mnemo_storage::Db;

use crate::blocking;

/// Handle for querying everything Mnemo has indexed (documents and
/// conversation history).
///
/// Obtained via [`crate::Mnemo::search`]; cheap to create (it just
/// holds a clone of the shared DB handle).
#[derive(Clone)]
pub struct SearchHandle {
    db: Db,
}

impl SearchHandle {
    pub(crate) fn new(db: Db) -> Self {
        Self { db }
    }

    /// Run a lexical (BM25) search with default options (all scopes,
    /// top 10 hits).
    pub async fn search(&self, query: impl Into<String> + Send + 'static) -> Result<Vec<SearchHit>> {
        self.search_with_options(query, SearchOptions::default()).await
    }

    /// Run a lexical (BM25) search scoped/limited per `options`.
    pub async fn search_with_options(
        &self,
        query: impl Into<String> + Send + 'static,
        options: SearchOptions,
    ) -> Result<Vec<SearchHit>> {
        let db = self.db.clone();
        blocking::run(move || Ok(mnemo_search::search(&db, &query.into(), &options)?)).await
    }

    /// Run semantic search: embed `query` with `embedder` and return
    /// the top `limit` chunks by cosine similarity against every
    /// stored embedding for that same `(model_name, model_version)`
    /// (plan.md section 6 "Vector Storage" / Phase 4).
    ///
    /// `embedder` typically comes from [`crate::EmbedHandle::embedder`]
    /// so the query is embedded with the same model used for
    /// [`crate::EmbedHandle::embed_pending`].
    pub async fn search_vector(
        &self,
        embedder: Arc<dyn Embedder>,
        query: impl Into<String> + Send + 'static,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let db = self.db.clone();
        blocking::run(move || Ok(mnemo_search::vector_search(&db, embedder.as_ref(), &query.into(), limit)?)).await
    }

    /// Run hybrid search: fuse lexical (BM25) and vector (cosine)
    /// candidates via weighted, min-max normalized score fusion
    /// (plan.md section 8 "Hybrid Retrieval" / Phase 5).
    pub async fn search_hybrid(
        &self,
        embedder: Arc<dyn Embedder>,
        query: impl Into<String> + Send + 'static,
        options: SearchOptions,
        weights: HybridWeights,
    ) -> Result<Vec<SearchHit>> {
        let db = self.db.clone();
        blocking::run(move || {
            Ok(mnemo_search::hybrid_search(
                &db,
                embedder.as_ref(),
                &query.into(),
                &options,
                weights,
            )?)
        })
        .await
    }
}
