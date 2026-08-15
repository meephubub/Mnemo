use sha2::{Digest, Sha256};

/// Content hash used for incremental indexing (plan.md section 15
/// "Incremental Indexing"): unchanged content hashes the same, so
/// re-ingesting a file that hasn't changed can be skipped.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
