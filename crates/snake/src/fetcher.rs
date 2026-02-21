use reqwest::Client;
use slither_core::{RawPage, SlitherError};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

pub struct Fetcher {
    pub client: Client,
}

impl Fetcher {
    pub fn new(user_agent: &str, request_timeout_secs: u64) -> Result<Self, SlitherError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout_secs))
            .user_agent(user_agent)
            .gzip(true)
            .brotli(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| SlitherError::Crawl(format!("failed to build HTTP client: {}", e)))?;

        Ok(Self { client })
    }

    pub async fn fetch(&self, url: &str) -> Option<RawPage> {
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("invalid URL {}: {}", url, e);
                return None;
            }
        };

        let domain = parsed.host_str().unwrap_or("").to_string();

        let response = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("fetch error for {}: {}", url, e);
                return None;
            }
        };

        let status = response.status().as_u16();

        if !response.status().is_success() {
            tracing::debug!("non-success status {} for {}", status, url);
            return None;
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.is_empty()
            && !content_type.contains("text/html")
            && !content_type.contains("application/xhtml")
        {
            tracing::debug!("skipping non-HTML content-type '{}' for {}", content_type, url);
            return None;
        }

        let mut headers = HashMap::new();
        for (name, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(name.as_str().to_string(), v.to_string());
            }
        }

        let html = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("failed to read body from {}: {}", url, e);
                return None;
            }
        };

        let crawled_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Some(RawPage {
            url: url.to_string(),
            domain,
            html,
            status,
            headers,
            crawled_at,
        })
    }
}
