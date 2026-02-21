use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Seed {
    Configured(SeedConfig),
    Simple(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeedConfig {
    pub url: String,
    #[serde(default = "default_seed_depth")]
    pub depth: usize,
    #[serde(default)]
    pub priority: SeedPriority,
    #[serde(default)]
    pub scope: CrawlScope,
    #[serde(default)]
    pub sitemap: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SeedPriority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrawlScope {
    #[default]
    Any,
    SameDomain,
    SameOrigin,
}

pub fn default_seed_depth() -> usize {
    3
}

impl Seed {
    pub fn url(&self) -> &str {
        match self {
            Seed::Configured(c) => &c.url,
            Seed::Simple(s) => s,
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Seed::Configured(c) => c.depth,
            Seed::Simple(_) => default_seed_depth(),
        }
    }

    pub fn priority(&self) -> SeedPriority {
        match self {
            Seed::Configured(c) => c.priority,
            Seed::Simple(_) => SeedPriority::Normal,
        }
    }

    pub fn scope(&self) -> CrawlScope {
        match self {
            Seed::Configured(c) => c.scope,
            Seed::Simple(_) => CrawlScope::Any,
        }
    }

    pub fn sitemap(&self) -> bool {
        match self {
            Seed::Configured(c) => c.sitemap,
            Seed::Simple(_) => false,
        }
    }
}

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
    #[serde(default)]
    pub favicon_url: Option<String>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    Text,
    Semantic,
    #[default]
    Hybrid,
}
