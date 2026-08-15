//! Bridges the synchronous storage layer (a single, mutex-guarded
//! SQLite connection) into the async public API described in
//! plan.md section 62 ("API Design").
//!
//! Retrieval is meant to stay fast/synchronous under the hood
//! (section 60 "Async Architecture"); `spawn_blocking` just keeps
//! that fast SQLite work off whatever async runtime the embedding
//! agent is using.

use mnemo_core::{MnemoError, Result};

pub(crate) async fn run<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| MnemoError::other(format!("background task panicked: {e}")))?
}
