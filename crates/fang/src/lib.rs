mod config;
mod extractor;
mod links;
mod text;

pub use config::FangConfig;
pub use slither_core::Document;

use scraper::Html;
use slither_core::{RawPage, SlitherError, SlitherResult};
use tracing::warn;
use xxhash_rust::xxh3::xxh3_64;

use extractor::{
    compute_content_hash, extract_headings, extract_lang, extract_meta_description, extract_title,
};
use links::extract_links;
use text::extract_clean_text;

pub struct Transformer;

impl Transformer {
    pub fn new() -> Self {
        Self
    }

    pub fn transform(&self, page: &RawPage) -> SlitherResult<Document> {
        Fang::new(FangConfig::default()).transform(page)
    }

    pub fn transform_batch(&self, pages: &[RawPage]) -> Vec<SlitherResult<Document>> {
        pages.iter().map(|p| self.transform(p)).collect()
    }
}

impl Default for Transformer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Fang {
    config: FangConfig,
}

impl Fang {
    pub fn new(config: FangConfig) -> Self {
        Self { config }
    }

    pub fn transform(&self, page: &RawPage) -> Result<Document, SlitherError> {
        if page.html.trim().is_empty() {
            return Err(SlitherError::Transform(format!(
                "empty HTML for url: {}",
                page.url
            )));
        }

        let content_type = page
            .headers
            .get("content-type")
            .or_else(|| page.headers.get("Content-Type"))
            .map(|s| s.as_str())
            .unwrap_or("text/html");

        let ct_lower = content_type.to_ascii_lowercase();
        let is_html = ct_lower.contains("text/html")
            || ct_lower.contains("application/xhtml");

        if !is_html {
            return Err(SlitherError::Transform(format!(
                "non-HTML content-type '{}' for url: {}",
                content_type, page.url
            )));
        }

        let document = Html::parse_document(&page.html);

        let title = extract_title(&document);
        let meta_description = extract_meta_description(&document);
        let headings = extract_headings(&document);
        let body = extract_clean_text(&document);

        if body.len() < self.config.min_content_length {
            warn!(
                url = %page.url,
                body_len = body.len(),
                min = self.config.min_content_length,
                "page body below minimum content length"
            );
        }

        let body = if body.len() > self.config.max_content_length {
            body[..self.config.max_content_length].to_string()
        } else {
            body
        };

        let lang = extract_lang(&document, &body);
        let content_hash = compute_content_hash(&body);

        let links = if self.config.extract_links {
            extract_links(&document, &page.url)
        } else {
            Vec::new()
        };

        let id = xxh3_64(page.url.as_bytes());

        Ok(Document {
            id,
            url: page.url.clone(),
            domain: page.domain.clone(),
            title,
            body,
            links,
            headings,
            meta_description,
            lang,
            content_hash,
            crawled_at: page.crawled_at,
        })
    }
}
