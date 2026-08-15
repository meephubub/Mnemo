//! `mnemo-embeddings` — embedding abstraction for Mnemo
//! (plan.md section 47 "Local Embedding Models" / Phase 4).
//!
//! Defines the [`Embedder`] trait and a default [`HashingEmbedder`]
//! that produces fixed-dimensional vectors via the hashing trick —
//! deterministic, dependency-free, and good enough to exercise the
//! vector/hybrid retrieval pipeline. Real local embedding models
//! (ONNX, Candle, LiteRT) can implement the same trait and slot in
//! without changing any callers.

pub mod error;

pub use error::{EmbedError, Result};

/// A model that converts text into a fixed-dimensional vector.
///
/// Implementations must be deterministic for the same `(model, text)`
/// pair so embeddings can be cached and re-computed on demand.
pub trait Embedder: Send + Sync {
    /// Human-readable model name (e.g. `"hashing"` or `"bge-small-en"`).
    fn model_name(&self) -> &str;

    /// Model version string, recorded alongside every embedding so
    /// incompatible vector indexes can be detected and rebuilt
    /// (plan.md section 49 "Model Versioning").
    fn model_version(&self) -> &str;

    /// Output dimensionality of this model's vectors.
    fn dimension(&self) -> usize;

    /// Embed a single piece of text.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in one call. Default implementation just
    /// loops; real models should batch for throughput.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// A deterministic, dependency-free embedder that uses the hashing
/// trick to project text into a fixed-dimensional space.
///
/// This is **not** a semantic embedding — it captures lexical
/// overlap, not meaning — but it is fast, local, and stable, which is
/// exactly what the retrieval pipeline needs to be exercised before
/// a real model is wired in. The vector is L2-normalised so cosine
/// similarity reduces to a dot product.
pub struct HashingEmbedder {
    dimension: usize,
    model_version: String,
}

impl HashingEmbedder {
    /// Create a hashing embedder with the given output dimension.
    /// 384 is a common small-model dimension and is the default.
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            model_version: "0.1.0".to_string(),
        }
    }

    pub fn default_dim() -> Self {
        Self::new(384)
    }
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self::default_dim()
    }
}

impl Embedder for HashingEmbedder {
    fn model_name(&self) -> &str {
        "hashing"
    }

    fn model_version(&self) -> &str {
        &self.model_version
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vec = vec![0.0_f32; self.dimension];
        for token in text.split_whitespace() {
            let token = token.to_lowercase();
            if token.is_empty() {
                continue;
            }
            let (idx, sign) = hash_token(&token, self.dimension);
            vec[idx] += sign;
        }
        l2_normalize(&mut vec);
        Ok(vec)
    }
}

fn hash_token(token: &str, dimension: usize) -> (usize, f32) {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in token.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let index = (h as usize) % dimension;
    let sign = if (h >> 63) & 1 == 0 { 1.0 } else { -1.0 };
    (index, sign)
}

fn l2_normalize(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_text() {
        let emb = HashingEmbedder::default_dim();
        let a = emb.embed("hello world").unwrap();
        let b = emb.embed("hello world").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn dimension_matches_config() {
        let emb = HashingEmbedder::new(128);
        let v = emb.embed("some text").unwrap();
        assert_eq!(v.len(), 128);
    }

    #[test]
    fn different_text_different_vector() {
        let emb = HashingEmbedder::default_dim();
        let a = emb.embed("rust programming").unwrap();
        let b = emb.embed("french cooking").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn vectors_are_l2_normalized() {
        let emb = HashingEmbedder::default_dim();
        let v = emb.embed("some longer text with many tokens here").unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_is_higher_for_overlapping_text() {
        let emb = HashingEmbedder::default_dim();
        let a = emb.embed("the rust programming language").unwrap();
        let b = emb.embed("the rust programming language is fast").unwrap();
        let c = emb.embed("completely different topic about cooking").unwrap();
        let sim_ab: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let sim_ac: f32 = a.iter().zip(&c).map(|(x, y)| x * y).sum();
        assert!(sim_ab > sim_ac, "overlapping text should be more similar: {sim_ab} vs {sim_ac}");
    }

    #[test]
    fn embed_batch_matches_individual() {
        let emb = HashingEmbedder::default_dim();
        let texts = ["alpha", "beta gamma"];
        let batch = emb.embed_batch(&texts).unwrap();
        let individual: Vec<_> = texts.iter().map(|t| emb.embed(t).unwrap()).collect();
        assert_eq!(batch, individual);
    }

    #[test]
    fn empty_text_yields_zero_vector() {
        let emb = HashingEmbedder::default_dim();
        let v = emb.embed("").unwrap();
        assert!(v.iter().all(|x| x.abs() < 1e-10));
    }
}
