use serde::{Deserialize, Serialize};

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
