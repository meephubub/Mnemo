//! Context packing API surface (plan.md section 11 "Context Packing"
//! / section 83 "Phase 7").
//!
//! Thin async wrapper over `mnemo-search::context::pack_context` so
//! it fits the same `spawn_blocking`-backed pattern as the rest of
//! the facade. Optional Stage 2 reranking (plan.md section 10 /
//! Phase 6) is available via [`ContextRequest::reranker`] /
//! [`ContextRequest::with_reranker`] or [`ContextHandle::pack_with_reranker`]
//! — see [`crate::Reranker`] / [`crate::HeuristicReranker`]. Optional
//! neighbor-chunk expansion ("preserve surrounding context where
//! needed") is available via [`ContextRequest::neighbor_expansion`] /
//! [`ContextRequest::with_neighbor_expansion`] or
//! [`ContextHandle::pack_with_neighbor_expansion`].

use std::sync::Arc;

pub use mnemo_search::{ContextChunk, ContextRequest, PackedContext};

use mnemo_core::Result;
use mnemo_embeddings::Embedder;
use mnemo_search::Reranker;
use mnemo_storage::Db;

use crate::blocking;

/// Handle for packing retrieval results into a token-budgeted context
/// suitable for feeding to an LLM prompt.
///
/// Obtained via [`crate::Mnemo::context`] / [`crate::Mnemo::context_with`];
/// cheap to create (holds a clone of the shared DB handle plus the
/// embedder to use for the vector half of retrieval).
#[derive(Clone)]
pub struct ContextHandle {
    db: Db,
    embedder: Arc<dyn Embedder>,
}

impl ContextHandle {
    pub(crate) fn new(db: Db, embedder: Arc<dyn Embedder>) -> Self {
        Self { db, embedder }
    }

    /// Pack a query into a token-budgeted context with default
    /// options (2000 token budget, up to 5 distinct sources — see
    /// [`ContextRequest::default`]).
    pub async fn pack(&self, query: impl Into<String> + Send + 'static, token_budget: usize) -> Result<PackedContext> {
        self.pack_with_request(ContextRequest::new(query.into(), token_budget)).await
    }

    /// Pack using a fully custom [`ContextRequest`] (weights, scope,
    /// max sources, candidate pool size, optional reranker, optional
    /// neighbor-chunk expansion).
    pub async fn pack_with_request(&self, request: ContextRequest) -> Result<PackedContext> {
        let db = self.db.clone();
        let embedder = self.embedder.clone();
        blocking::run(move || Ok(mnemo_search::pack_context(&db, embedder.as_ref(), &request)?)).await
    }

    /// Pack a query into a token-budgeted context, running `reranker`
    /// as an optional Stage 2 over the fused candidate pool before
    /// dedup/packing (plan.md section 10 / Phase 6). Otherwise
    /// identical to [`Self::pack`] (default token budget options
    /// aside from the reranker).
    pub async fn pack_with_reranker(
        &self,
        query: impl Into<String> + Send + 'static,
        token_budget: usize,
        reranker: Arc<dyn Reranker>,
    ) -> Result<PackedContext> {
        self.pack_with_request(ContextRequest::new(query.into(), token_budget).with_reranker(reranker))
            .await
    }

    /// Pack a query into a token-budgeted context with neighbor-chunk
    /// expansion enabled: each selected document chunk's immediate
    /// previous/next sibling is pulled in too when the token budget
    /// allows (plan.md's "preserve surrounding context where
    /// needed"). Otherwise identical to [`Self::pack`].
    pub async fn pack_with_neighbor_expansion(
        &self,
        query: impl Into<String> + Send + 'static,
        token_budget: usize,
    ) -> Result<PackedContext> {
        self.pack_with_request(ContextRequest::new(query.into(), token_budget).with_neighbor_expansion(true))
            .await
    }
}
