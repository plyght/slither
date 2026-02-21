use dashmap::DashMap;
use reqwest::Client;
use slither_core::{RawPage, SlitherError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Clone, Debug)]
struct CachedHeaders {
    etag: Option<String>,
    last_modified: Option<String>,
}

pub struct Fetcher {
    pub client: Client,
    cache: Arc<DashMap<String, CachedHeaders>>,
}

impl Fetcher {
    pub fn new(user_agent: &str, request_timeout_secs: u64) -> Result<Self, SlitherError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout_secs))
            .user_agent(user_agent)
            .gzip(true)
            .brotli(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(100)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SlitherError::Crawl(format!("failed to build HTTP client: {}", e)))?;

        Ok(Self { client, cache: Arc::new(DashMap::new()) })
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

        let mut request = self.client.get(url);
        if let Some(cached) = self.cache.get(url) {
            if let Some(ref etag) = cached.etag {
                request = request.header("If-None-Match", etag.as_str());
            }
            if let Some(ref lm) = cached.last_modified {
                request = request.header("If-Modified-Since", lm.as_str());
            }
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("fetch error for {}: {}", url, e);
                return None;
            }
        };

        let status = response.status().as_u16();

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            tracing::debug!("304 Not Modified for {}", url);
            return None;
        }

        if !response.status().is_success() {
            tracing::debug!("non-success status {} for {}", status, url);
            return None;
        }

        let etag = response.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
        let last_modified = response.headers().get("last-modified").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
        if etag.is_some() || last_modified.is_some() {
            self.cache.insert(url.to_string(), CachedHeaders { etag, last_modified });
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
