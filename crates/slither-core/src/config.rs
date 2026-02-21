use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlitherConfig {
    pub crawler: CrawlerConfig,
    pub index: IndexConfig,
    pub embedder: EmbedderConfig,
    pub storage: StorageConfig,
    pub data_dir: String,
    #[serde(default)]
    pub admin_key: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub max_gb: Option<f64>,
    pub warning_threshold_gb: f64,
    pub check_interval_secs: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_gb: None,
            warning_threshold_gb: 10.0,
            check_interval_secs: 60,
        }
    }
}

impl Default for SlitherConfig {
    fn default() -> Self {
        Self {
            crawler: CrawlerConfig::default(),
            index: IndexConfig::default(),
            embedder: EmbedderConfig::default(),
            storage: StorageConfig::default(),
            data_dir: "slither_data".to_string(),
            admin_key: None,
        }
    }
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 50,
            max_depth: 3,
            rate_limit_per_second: 10,
            user_agent: "SlitherBot/0.1 (+https://search.peril.lol/about)".to_string(),
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
