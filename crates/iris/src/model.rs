use ndarray::Array2;
use ort::{session::Session, value::TensorRef};
use parking_lot::Mutex;
use slither_core::{EmbedderConfig, SlitherError};
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{info, warn};

enum ModelInner {
    Onnx {
        session: Mutex<Session>,
        tokenizer: Box<Tokenizer>,
    },
    Fallback,
}

pub(crate) struct EmbeddingModel {
    inner: ModelInner,
    embedding_dim: usize,
}

impl EmbeddingModel {
    /// Load the ONNX session and HuggingFace tokenizer.
    ///
    /// Falls back to deterministic pseudo-random vectors when the model files
    /// are missing so the rest of the pipeline can be exercised without
    /// downloading weights.
    ///
    /// Download from:
    ///   https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2
    ///   Required: `all-MiniLM-L6-v2.onnx` and `tokenizer.json`
    pub fn load(config: &EmbedderConfig) -> Result<Self, SlitherError> {
        let model_path = Path::new(&config.model_path);
        let model_dir = model_path.parent().unwrap_or(Path::new("."));
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            warn!(
                "Model files not found ({:?} / {:?}). \
                 Using pseudo-random fallback embeddings. \
                 Download from https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2",
                model_path, tokenizer_path,
            );
            return Ok(Self {
                inner: ModelInner::Fallback,
                embedding_dim: config.dimensions,
            });
        }

        let intra_threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(2))
            .unwrap_or(4);

        let session = Session::builder()
            .map_err(|e| SlitherError::Embedding(e.to_string()))?
            .with_intra_threads(intra_threads)
            .map_err(|e| SlitherError::Embedding(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| SlitherError::Embedding(e.to_string()))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| SlitherError::Embedding(e.to_string()))?;

        info!(
            "Loaded ONNX embedding model from {:?} (intra_threads={})",
            model_path, intra_threads
        );

        Ok(Self {
            inner: ModelInner::Onnx {
                session: Mutex::new(session),
                tokenizer: Box::new(tokenizer),
            },
            embedding_dim: config.dimensions,
        })
    }

    pub fn is_fallback(&self) -> bool {
        matches!(self.inner, ModelInner::Fallback)
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, SlitherError> {
        match &self.inner {
            ModelInner::Fallback => Ok(fallback_embed(text, self.embedding_dim)),
            ModelInner::Onnx { session, tokenizer } => {
                embed_onnx(text, session, tokenizer, self.embedding_dim)
            }
        }
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, SlitherError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if texts.len() == 1 {
            return Ok(vec![self.embed(texts[0])?]);
        }
        match &self.inner {
            ModelInner::Fallback => texts
                .iter()
                .map(|t| Ok(fallback_embed(t, self.embedding_dim)))
                .collect(),
            ModelInner::Onnx { session, tokenizer } => {
                embed_batch_onnx(texts, session, tokenizer, self.embedding_dim)
            }
        }
    }
}

fn embed_batch_onnx(
    texts: &[&str],
    session: &Mutex<Session>,
    tokenizer: &Tokenizer,
    embedding_dim: usize,
) -> Result<Vec<Vec<f32>>, SlitherError> {
    let encodings: Vec<_> = texts
        .iter()
        .map(|t| {
            tokenizer
                .encode(*t, true)
                .map_err(|e| SlitherError::Embedding(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let batch_size = encodings.len();
    let max_len = encodings
        .iter()
        .map(|e| e.get_ids().len())
        .max()
        .unwrap_or(0);

    if max_len == 0 {
        return Ok(vec![vec![0.0; embedding_dim]; batch_size]);
    }

    let mut input_ids = vec![0i64; batch_size * max_len];
    let mut attention_masks = vec![0i64; batch_size * max_len];
    let mut token_type_ids_flat = vec![0i64; batch_size * max_len];

    for (i, enc) in encodings.iter().enumerate() {
        let ids = enc.get_ids();
        let mask = enc.get_attention_mask();
        let types = enc.get_type_ids();
        let row = i * max_len;
        for (j, (&id, (&m, &t))) in ids.iter().zip(mask.iter().zip(types.iter())).enumerate() {
            input_ids[row + j] = id as i64;
            attention_masks[row + j] = m as i64;
            token_type_ids_flat[row + j] = t as i64;
        }
    }

    let ids_arr = Array2::from_shape_vec((batch_size, max_len), input_ids)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let mask_arr = Array2::from_shape_vec((batch_size, max_len), attention_masks)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let types_arr = Array2::from_shape_vec((batch_size, max_len), token_type_ids_flat)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let ids_tensor = TensorRef::from_array_view(ids_arr.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let mask_tensor = TensorRef::from_array_view(mask_arr.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let types_tensor = TensorRef::from_array_view(types_arr.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let mut guard = session.lock();
    let outputs = guard
        .run(ort::inputs![
            "input_ids"      => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => types_tensor,
        ])
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let tensor = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let shape = tensor.shape().to_vec();
    if shape.len() != 3 {
        return Err(SlitherError::Embedding(format!(
            "unexpected output rank {}, want 3 [batch, seq, dim]",
            shape.len()
        )));
    }
    let out_seq = shape[1];
    let out_dim = shape[2];

    let flat = tensor
        .as_slice()
        .ok_or_else(|| SlitherError::Embedding("output tensor is not contiguous".into()))?;

    let mut results = Vec::with_capacity(batch_size);
    for i in 0..batch_size {
        let mask_f32: Vec<f32> = encodings[i]
            .get_attention_mask()
            .iter()
            .map(|&x| x as f32)
            .collect();
        let mask_sum: f32 = mask_f32.iter().sum::<f32>().max(1e-9);

        let mut pooled = vec![0.0f32; out_dim];
        for j in 0..out_seq {
            let m = mask_f32.get(j).copied().unwrap_or(0.0);
            if m == 0.0 {
                continue;
            }
            let base = (i * out_seq + j) * out_dim;
            for k in 0..out_dim {
                pooled[k] += flat[base + k] * m;
            }
        }
        for v in pooled.iter_mut() {
            *v /= mask_sum;
        }

        crate::search::l2_normalize(&mut pooled);
        pooled.resize(embedding_dim, 0.0);
        results.push(pooled);
    }

    Ok(results)
}

fn embed_onnx(
    text: &str,
    session: &Mutex<Session>,
    tokenizer: &Tokenizer,
    embedding_dim: usize,
) -> Result<Vec<f32>, SlitherError> {
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
    let mask_i64: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|&x| x as i64)
        .collect();
    let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();
    let seq_len = ids.len();

    let input_ids = Array2::from_shape_vec((1, seq_len), ids)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let attention_mask = Array2::from_shape_vec((1, seq_len), mask_i64)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let token_type_ids = Array2::from_shape_vec((1, seq_len), type_ids)
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let input_ids_tensor = TensorRef::from_array_view(input_ids.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let attention_mask_tensor = TensorRef::from_array_view(attention_mask.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;
    let token_type_ids_tensor = TensorRef::from_array_view(token_type_ids.view())
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let mut guard = session.lock();
    let outputs = guard
        .run(ort::inputs![
            "input_ids"      => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let tensor = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| SlitherError::Embedding(e.to_string()))?;

    let shape = tensor.shape().to_vec();
    if shape.len() != 3 {
        return Err(SlitherError::Embedding(format!(
            "unexpected output rank {}, want 3 [batch, seq, dim]",
            shape.len()
        )));
    }
    let out_seq = shape[1];
    let out_dim = shape[2];

    let flat = tensor
        .as_slice()
        .ok_or_else(|| SlitherError::Embedding("output tensor is not contiguous".into()))?;

    let mask_f32: Vec<f32> = encoding
        .get_attention_mask()
        .iter()
        .map(|&x| x as f32)
        .collect();
    let mask_sum: f32 = mask_f32.iter().sum::<f32>().max(1e-9);

    let mut pooled = vec![0.0f32; out_dim];
    for j in 0..out_seq {
        let m = mask_f32.get(j).copied().unwrap_or(0.0);
        if m == 0.0 {
            continue;
        }
        let row = j * out_dim;
        for k in 0..out_dim {
            pooled[k] += flat[row + k] * m;
        }
    }
    for v in pooled.iter_mut() {
        *v /= mask_sum;
    }

    crate::search::l2_normalize(&mut pooled);

    pooled.resize(embedding_dim, 0.0);
    Ok(pooled)
}

fn fallback_embed(text: &str, dim: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    let seed = h.finish();

    let mut vec: Vec<f32> = (0..dim)
        .map(|i| {
            let mut h2 = DefaultHasher::new();
            seed.wrapping_add(i as u64)
                .wrapping_mul(6364136223846793005u64)
                .hash(&mut h2);
            let v = h2.finish();
            (v as f32 / u64::MAX as f32) * 2.0 - 1.0
        })
        .collect();

    crate::search::l2_normalize(&mut vec);
    vec
}
