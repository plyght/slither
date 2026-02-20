use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlitherConfig {
    pub crawler: CrawlerConfig,
    pub index: IndexConfig,
    pub embedder: EmbedderConfig,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    pub max_concurrent: usize,
    pub max_depth: usize,
    pub rate_limit_per_second: u32,
    pub user_agent: String,
    pub respect_robots: bool,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedderConfig {
    pub model_path: String,
    pub dimensions: usize,
    pub data_dir: String,
}

impl Default for SlitherConfig {
    fn default() -> Self {
        Self {
            crawler: CrawlerConfig::default(),
            index: IndexConfig::default(),
            embedder: EmbedderConfig::default(),
            data_dir: "slither_data".to_string(),
        }
    }
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 50,
            max_depth: 3,
            rate_limit_per_second: 10,
            user_agent: "SlitherBot/0.1".to_string(),
            respect_robots: true,
            request_timeout_secs: 30,
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            data_dir: "slither_data/index".to_string(),
        }
    }
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model_path: "models/all-MiniLM-L6-v2.onnx".to_string(),
            dimensions: 384,
            data_dir: "slither_data/vectors".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisConfig {
    pub model_path: PathBuf,
    pub embedding_dim: usize,
}

impl Default for IrisConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/all-MiniLM-L6-v2.onnx"),
            embedding_dim: 384,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FangConfig {
    pub min_content_length: usize,
    pub max_content_length: usize,
    pub extract_links: bool,
}

impl Default for FangConfig {
    fn default() -> Self {
        Self {
            min_content_length: 50,
            max_content_length: 1_000_000,
            extract_links: true,
        }
    }
}
