//! Search API surface (plan.md section 7 "Full-Text Search" / Phase 3).
//!
//! Thin async wrapper over `mnemo-search`'s synchronous lexical
//! search so it fits the same `spawn_blocking`-backed pattern as the
//! rest of the facade. Vector/hybrid search lands in later phases —
//! see ROADMAP.md — without needing to change this signature.

pub use mnemo_search::{HitKind, SearchHit, SearchOptions, SearchScope};

use mnemo_core::Result;
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
}
