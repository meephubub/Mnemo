//! Real local sentence-embedding model, backed by [Candle] (a
//! pure-Rust ML tensor/inference framework — no Python, no ONNX
//! Runtime, no system BLAS/ML library required) running a
//! `sentence-transformers`-style BERT encoder (plan.md section 47
//! "Local Embedding Models" / ROADMAP.md Phase 4 follow-up
//! "replace `HashingEmbedder` with a real local embedding model").
//!
//! [`BertEmbedder::new`] downloads the model's config, tokenizer, and
//! weights from the Hugging Face Hub via [`hf-hub`] the first time
//! it's called, caching them under `~/.cache/huggingface/hub` (or
//! `$HF_HOME` if set) exactly like Python's `huggingface_hub`/
//! `sentence-transformers` do — every call after the first (even in a
//! later process, even on a later day) reuses the cached files and
//! does not touch the network again. No separate download script or
//! setup step exists or is needed; adding this crate as a dependency
//! and constructing a [`BertEmbedder`] is the entire setup.
//!
//! [Candle]: https://github.com/huggingface/candle

use std::sync::Mutex;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, HiddenAct, DTYPE};
use hf_hub::api::sync::Api;
use hf_hub::{Repo, RepoType};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer};

use crate::error::{EmbedError, Result};
use crate::Embedder;

/// The default model: a small, widely used `sentence-transformers`
/// model that maps text to a 384-dimensional space, good enough for
/// general-purpose semantic search while staying small enough to run
/// comfortably on CPU (which is all Candle is configured to use
/// here — see [`BertEmbedder::new`]).
pub const DEFAULT_MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// `main` carries a `model.safetensors` file on this repo as of this
/// writing; [`BertEmbedder::from_pretrained`] falls back to
/// `pytorch_model.bin` automatically if a future revision doesn't.
pub const DEFAULT_REVISION: &str = "main";

/// A real local sentence-embedding model: a BERT encoder run through
/// [Candle](https://github.com/huggingface/candle), mean-pooled over
/// non-padding tokens and L2-normalized, matching the
/// `sentence-transformers` library's own pooling convention so
/// vectors produced here are directly comparable (via cosine
/// similarity) to ones produced by the Python library for the same
/// model.
///
/// CPU-only by construction ([`Device::Cpu`]) — this is meant to run
/// anywhere Mnemo runs, without requiring a GPU or any GPU driver
/// setup. `BertModel::forward` takes `&self` and `BertModel` is
/// `Send + Sync`, so it needs no synchronization to satisfy
/// [`Embedder`]'s `&self`-based methods; `Tokenizer`, however, needs
/// `&mut self` to toggle padding between single-text and batch calls
/// (see [`Self::embed_batch`]), so it's kept behind a [`Mutex`].
pub struct BertEmbedder {
    model: BertModel,
    tokenizer: Mutex<Tokenizer>,
    dimension: usize,
    model_name: String,
    model_version: String,
}

impl BertEmbedder {
    /// Load [`DEFAULT_MODEL_ID`] at [`DEFAULT_REVISION`]. This is the
    /// call sufficient to satisfy the "real local embedding model"
    /// requirement — see the module docs for exactly what happens
    /// (and when) on first use.
    pub fn new() -> Result<Self> {
        Self::from_pretrained(DEFAULT_MODEL_ID, DEFAULT_REVISION)
    }

    /// Load any `sentence-transformers`-compatible BERT model from
    /// the Hugging Face Hub by repo id (e.g.
    /// `"sentence-transformers/all-MiniLM-L6-v2"`) and revision
    /// (branch, tag, or commit; `"main"` for the default branch).
    ///
    /// Downloads (on first use only — see module docs) `config.json`,
    /// `tokenizer.json`, and the model weights (`model.safetensors`
    /// if present on `revision`, otherwise `pytorch_model.bin`) via
    /// `hf-hub`, which handles the actual caching to disk; nothing in
    /// this crate re-implements or bypasses that cache.
    pub fn from_pretrained(model_id: &str, revision: &str) -> Result<Self> {
        let device = Device::Cpu;

        let api = Api::new().map_err(|e| EmbedError::Model(format!("failed to initialize Hugging Face Hub API client: {e}")))?;
        let repo = api.repo(Repo::with_revision(model_id.to_string(), RepoType::Model, revision.to_string()));

        let config_path = repo
            .get("config.json")
            .map_err(|e| EmbedError::Model(format!("failed to fetch config.json for {model_id}@{revision}: {e}")))?;
        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| EmbedError::Model(format!("failed to fetch tokenizer.json for {model_id}@{revision}: {e}")))?;

        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| EmbedError::Model(format!("failed to read downloaded config.json: {e}")))?;
        let mut config: Config = serde_json::from_str(&config_str)
            .map_err(|e| EmbedError::Model(format!("failed to parse config.json for {model_id}@{revision}: {e}")))?;
        // `sentence-transformers` models commonly use the tanh-based
        // GELU approximation rather than the exact erf-based one;
        // matching it keeps output numerically aligned with the
        // Python library for the same model. HiddenAct is read from
        // config.json when present, so this is only a fallback for
        // configs that leave it at BERT's original default.
        if config.hidden_act == HiddenAct::Gelu {
            config.hidden_act = HiddenAct::GeluApproximate;
        }

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedError::Model(format!("failed to load tokenizer for {model_id}@{revision}: {e}")))?;
        // No padding for single-text `embed()` (each call encodes
        // exactly one sequence, so there's nothing to pad against);
        // `embed_batch` overrides this to batch-longest padding right
        // before encoding a batch, then restores this setting.
        tokenizer.with_padding(None).with_truncation(None).map_err(|e| EmbedError::Model(e.to_string()))?;

        let dimension = config.hidden_size;
        let model_version = format!("{model_id}@{revision}");

        let vb = match repo.get("model.safetensors") {
            Ok(weights_path) => unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)
                    .map_err(|e| EmbedError::Model(format!("failed to load model.safetensors for {model_id}@{revision}: {e}")))?
            },
            Err(_) => {
                // Fall back to the PyTorch weights for repos that
                // don't publish a `model.safetensors` file on this
                // revision (older snapshots of some models).
                let weights_path = repo.get("pytorch_model.bin").map_err(|e| {
                    EmbedError::Model(format!(
                        "failed to fetch model weights for {model_id}@{revision} (tried model.safetensors and pytorch_model.bin): {e}"
                    ))
                })?;
                VarBuilder::from_pth(&weights_path, DTYPE, &device)
                    .map_err(|e| EmbedError::Model(format!("failed to load pytorch_model.bin for {model_id}@{revision}: {e}")))?
            }
        };

        let model = BertModel::load(vb, &config)
            .map_err(|e| EmbedError::Model(format!("failed to build BERT model for {model_id}@{revision}: {e}")))?;

        Ok(Self {
            model,
            tokenizer: Mutex::new(tokenizer),
            dimension,
            model_name: model_id.to_string(),
            model_version,
        })
    }
}

impl Embedder for BertEmbedder {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn model_version(&self) -> &str {
        &self.model_version
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.embed_batch(&[text])?.into_iter().next().unwrap_or_default())
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let device = Device::Cpu;
        let mut tokenizer = self.tokenizer.lock().map_err(|_| EmbedError::Model("tokenizer mutex poisoned".to_string()))?;

        // Pad every sequence in this batch up to the longest one so
        // they can be stacked into a single tensor, then restore the
        // no-padding setting used by single-text calls.
        tokenizer.with_padding(Some(PaddingParams { strategy: PaddingStrategy::BatchLongest, ..Default::default() }));
        let encodings = tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| EmbedError::Model(format!("tokenization failed: {e}")))?;
        tokenizer.with_padding(None);

        let token_ids: Vec<Tensor> = encodings
            .iter()
            .map(|enc| Tensor::new(enc.get_ids(), &device))
            .collect::<candle_core::Result<_>>()
            .map_err(|e| EmbedError::Model(e.to_string()))?;
        let attention_mask: Vec<Tensor> = encodings
            .iter()
            .map(|enc| Tensor::new(enc.get_attention_mask(), &device))
            .collect::<candle_core::Result<_>>()
            .map_err(|e| EmbedError::Model(e.to_string()))?;

        let token_ids = Tensor::stack(&token_ids, 0).map_err(|e| EmbedError::Model(e.to_string()))?;
        let attention_mask = Tensor::stack(&attention_mask, 0).map_err(|e| EmbedError::Model(e.to_string()))?;
        let token_type_ids = token_ids.zeros_like().map_err(|e| EmbedError::Model(e.to_string()))?;

        let hidden_states = self
            .model
            .forward(&token_ids, &token_type_ids, Some(&attention_mask))
            .map_err(|e| EmbedError::Model(format!("BERT forward pass failed: {e}")))?;

        let pooled = mean_pool(&hidden_states, &attention_mask).map_err(|e| EmbedError::Model(e.to_string()))?;
        let normalized = normalize_l2(&pooled).map_err(|e| EmbedError::Model(e.to_string()))?;

        normalized.to_vec2::<f32>().map_err(|e| EmbedError::Model(format!("failed to read embeddings out of tensor: {e}")).into())
    }
}

/// Mean-pool `hidden` (`[batch, seq_len, hidden_size]`) over the
/// sequence dimension, weighting each token by `attention_mask`
/// (`[batch, seq_len]`) so padding tokens contribute nothing to the
/// average — the same pooling `sentence-transformers` uses for BERT-
/// family models, which is why matching it keeps embeddings
/// comparable to ones produced by the Python library.
fn mean_pool(hidden: &Tensor, attention_mask: &Tensor) -> candle_core::Result<Tensor> {
    let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?;
    let sum_mask = mask.sum(1)?;
    let summed = hidden.broadcast_mul(&mask)?.sum(1)?;
    summed.broadcast_div(&sum_mask)
}

/// L2-normalize each row of `v` (`[batch, hidden_size]`) so cosine
/// similarity between any two rows reduces to a plain dot product,
/// matching every other [`Embedder`] in this crate.
fn normalize_l2(v: &Tensor) -> candle_core::Result<Tensor> {
    v.broadcast_div(&v.sqr()?.sum_keepdim(1)?.sqrt()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests download real model weights from the Hugging Face
    // Hub on first run (cached afterwards) and are therefore gated
    // behind an env var rather than run unconditionally as part of
    // `cargo test` — CI environments without network access (or
    // where a multi-hundred-MB download is undesirable on every run)
    // should not be broken by them. Run with:
    //   MNEMO_TEST_BERT_EMBEDDER=1 cargo test -p mnemo-embeddings bert::
    fn network_tests_enabled() -> bool {
        std::env::var("MNEMO_TEST_BERT_EMBEDDER").is_ok()
    }

    #[test]
    fn embeds_deterministically_and_normalized() {
        if !network_tests_enabled() {
            eprintln!("skipping: set MNEMO_TEST_BERT_EMBEDDER=1 to run (downloads model weights)");
            return;
        }
        let embedder = BertEmbedder::new().expect("load default model");
        let a = embedder.embed("The cat sits outside").expect("embed");
        let b = embedder.embed("The cat sits outside").expect("embed");
        assert_eq!(a, b, "same input should produce identical output");
        assert_eq!(a.len(), embedder.dimension());

        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "expected L2-normalized output, got norm {norm}");
    }

    #[test]
    fn semantically_similar_sentences_rank_closer_than_unrelated() {
        if !network_tests_enabled() {
            eprintln!("skipping: set MNEMO_TEST_BERT_EMBEDDER=1 to run (downloads model weights)");
            return;
        }
        let embedder = BertEmbedder::new().expect("load default model");
        let cat_a = embedder.embed("The cat sits outside").expect("embed");
        let cat_b = embedder.embed("A cat is sitting in the garden").expect("embed");
        let unrelated = embedder.embed("The stock market fell sharply today").expect("embed");

        let sim = |a: &[f32], b: &[f32]| -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() };
        let sim_cats = sim(&cat_a, &cat_b);
        let sim_unrelated = sim(&cat_a, &unrelated);
        assert!(sim_cats > sim_unrelated, "expected {sim_cats} > {sim_unrelated}");
    }

    #[test]
    fn embed_batch_matches_individual_embed() {
        if !network_tests_enabled() {
            eprintln!("skipping: set MNEMO_TEST_BERT_EMBEDDER=1 to run (downloads model weights)");
            return;
        }
        let embedder = BertEmbedder::new().expect("load default model");
        let texts = ["hello world", "a completely different sentence about cooking pasta"];
        let batch = embedder.embed_batch(&texts).expect("embed_batch");
        for (text, batch_vec) in texts.iter().zip(&batch) {
            let individual = embedder.embed(text).expect("embed");
            for (a, b) in individual.iter().zip(batch_vec) {
                assert!((a - b).abs() < 1e-3, "batch and individual embeddings should match closely");
            }
        }
    }
}
