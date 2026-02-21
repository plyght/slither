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
    compute_content_hash, extract_favicon_url, extract_headings, extract_lang,
    extract_meta_description, extract_title,
};
use links::extract_links;
pub use text::content_quality_score;
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

fn is_homepage(url: &str) -> bool {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let path = match without_scheme.find('/') {
        Some(i) => &without_scheme[i..],
        None => "",
    };
    let path_clean = path.split('?').next().unwrap_or(path);
    let path_clean = path_clean.split('#').next().unwrap_or(path_clean);
    path_clean.is_empty()
        || path_clean == "/"
        || path_clean == "/index.html"
        || path_clean == "/index.htm"
}

fn extract_domain_name(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = host.split(':').next().unwrap_or(host);
    let bare = host.strip_prefix("www.").unwrap_or(host);
    bare.to_string()
}

fn enrich_homepage_body(
    body: &str,
    title: &str,
    meta_description: &Option<String>,
    headings: &[String],
    url: &str,
) -> String {
    if !is_homepage(url) {
        return body.to_string();
    }

    let domain = extract_domain_name(url);
    let domain_name = domain.split('.').next().unwrap_or(&domain);

    let word_count = body.split_whitespace().count();
    if word_count >= 100 {
        return body.to_string();
    }

    let mut enriched = String::with_capacity(body.len() + 512);

    enriched.push_str(domain_name);
    enriched.push_str(" - ");
    enriched.push_str(&domain);

    if !title.is_empty() {
        enriched.push_str("\n\n");
        enriched.push_str(title);
    }

    if let Some(desc) = meta_description {
        if !desc.is_empty() {
            enriched.push_str("\n\n");
            enriched.push_str(desc);
        }
    }

    if !headings.is_empty() {
        enriched.push_str("\n\n");
        for h in headings.iter().take(10) {
            enriched.push_str(h);
            enriched.push_str(". ");
        }
    }

    if !body.is_empty() {
        enriched.push_str("\n\n");
        enriched.push_str(body);
    }

    enriched
}

#[derive(Clone)]
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
        let is_html = ct_lower.contains("text/html") || ct_lower.contains("application/xhtml");

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
        let mut body = extract_clean_text(&document);

        if body.len() < self.config.min_content_length {
            if let Some(meta) = &meta_description {
                if !meta.is_empty() {
                    body = meta.clone();
                }
            }
            if body.len() < self.config.min_content_length {
                warn!(
                    url = %page.url,
                    body_len = body.len(),
                    min = self.config.min_content_length,
                    "page body below minimum content length"
                );
            }
        }

        let body = if body.len() > self.config.max_content_length {
            body[..self.config.max_content_length].to_string()
        } else {
            body
        };

        let body = enrich_homepage_body(&body, &title, &meta_description, &headings, &page.url);

        let lang = extract_lang(&document, &body);
        let content_hash = compute_content_hash(&body);
        let favicon_url = extract_favicon_url(&document, &page.url);

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
            favicon_url,
        })
    }
}
