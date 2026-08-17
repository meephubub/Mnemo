//! Small, dependency-free vector math shared by any crate that needs
//! to compare embedding vectors (`mnemo-search`'s vector/hybrid
//! search, and the facade's contradiction detection over memory
//! content — plan.md section 29). Lives here rather than in
//! `mnemo-search` or `mnemo-embeddings` so both can use the exact
//! same similarity definition without one depending on the other.

/// Cosine similarity between two vectors, in `[-1.0, 1.0]`.
///
/// Returns `0.0` for mismatched lengths, empty vectors, or either
/// input being the zero vector (rather than `NaN` from a `0.0 / 0.0`
/// division), since "no similarity" is a more useful default for
/// ranking/thresholding callers than a value that poisons comparisons.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    // Embeddings produced by `mnemo-embeddings::Embedder` implementations
    // are L2-normalized, so cosine similarity reduces to a dot product.
    // Compute the full cosine formula anyway so this holds for any
    // future `Embedder` that doesn't normalize its output.
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_have_similarity_one() {
        let v = vec![0.6, 0.8];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_have_similarity_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors_have_similarity_negative_one() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn mismatched_lengths_yield_zero() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0]), 0.0);
    }

    #[test]
    fn zero_vector_yields_zero_instead_of_nan() {
        let zero = vec![0.0, 0.0];
        let other = vec![1.0, 1.0];
        assert_eq!(cosine_similarity(&zero, &other), 0.0);
    }
}
