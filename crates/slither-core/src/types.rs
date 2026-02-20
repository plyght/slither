use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPage {
    pub url: String,
    pub domain: String,
    pub html: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub crawled_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub url: String,
    pub domain: String,
    pub title: String,
    pub body: String,
    pub links: Vec<String>,
    pub headings: Vec<String>,
    pub meta_description: Option<String>,
    pub lang: Option<String>,
    pub content_hash: u64,
    pub crawled_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub doc_id: u64,
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    Text,
    Semantic,
    Hybrid,
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Hybrid
    }
}
