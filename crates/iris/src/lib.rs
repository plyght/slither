mod model;
mod search;
mod storage;

use model::EmbeddingModel;
use parking_lot::RwLock;
use slither_core::{EmbedderConfig, SlitherError};
use storage::VectorStorage;

pub struct Iris {
    model: RwLock<EmbeddingModel>,
    storage: RwLock<VectorStorage>,
}

impl Iris {
    pub fn new(config: EmbedderConfig) -> Result<Self, SlitherError> {
        let model = EmbeddingModel::load(&config)?;
        let storage = VectorStorage::open(&config)?;
        Ok(Self {
            model: RwLock::new(model),
            storage: RwLock::new(storage),
        })
    }

    pub fn is_fallback(&self) -> bool {
        self.model.read().is_fallback()
    }

    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, SlitherError> {
        self.model.read().embed(text)
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, SlitherError> {
        self.model.read().embed_batch(texts)
    }

    pub fn store_vector(&mut self, doc_id: u64, vector: &[f32]) -> Result<(), SlitherError> {
        self.storage.write().store(doc_id, vector)
    }

    pub fn store_vectors_batch(
        &mut self,
        entries: &[(u64, Vec<f32>)],
    ) -> Result<(), SlitherError> {
        self.storage.write().store_batch(entries)
    }

    pub fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(u64, f32)>, SlitherError> {
        self.storage.read().search(query_vector, limit)
    }

    pub fn search_filtered(
        &self,
        query_vector: &[f32],
        limit: usize,
        allowed_ids: &std::collections::HashSet<u64>,
    ) -> Result<Vec<(u64, f32)>, SlitherError> {
        self.storage
            .read()
            .search_filtered(query_vector, limit, allowed_ids)
    }

    pub fn vector_count(&self) -> u64 {
        self.storage.read().vector_count()
    }
}
