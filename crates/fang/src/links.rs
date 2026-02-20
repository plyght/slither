use scraper::{Html, Selector};
use url::Url;

const BLOCKED_SCHEMES: &[&str] = &["javascript", "mailto", "tel", "data", "ftp", "file"];

pub fn extract_links(document: &Html, base_url: &str) -> Vec<String> {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };

    let selector = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut links: Vec<String> = Vec::new();

    for el in document.select(&selector) {
        let href = match el.value().attr("href") {
            Some(h) => h.trim(),
            None => continue,
        };

        if href.is_empty() || href.starts_with('#') {
            continue;
        }

        let scheme_lower = href.to_lowercase();
        if BLOCKED_SCHEMES.iter().any(|s| scheme_lower.starts_with(s)) {
            continue;
        }

        let resolved = match base.join(href) {
            Ok(u) => u,
            Err(_) => continue,
        };

        if BLOCKED_SCHEMES.contains(&resolved.scheme()) {
            continue;
        }

        let normalized = normalize_url(resolved);
        if !normalized.is_empty() && !links.contains(&normalized) {
            links.push(normalized);
        }
    }

    links
}

fn normalize_url(mut url: Url) -> String {
    url.set_fragment(None);

    let scheme = url.scheme().to_lowercase();
    let host = url.host_str().unwrap_or("").to_lowercase();

    if host.is_empty() {
        return String::new();
    }

    let path = url.path().to_string();
    let path = if path.len() > 1 && path.ends_with('/') {
        path.trim_end_matches('/').to_string()
    } else {
        path
    };

    let port_str = match url.port() {
        Some(p) => {
            let default_port = match scheme.as_str() {
                "http" => Some(80u16),
                "https" => Some(443u16),
                _ => None,
            };
            if default_port == Some(p) {
                String::new()
            } else {
                format!(":{}", p)
            }
        }
        None => String::new(),
    };

    let query = url.query().map(|q| format!("?{}", q)).unwrap_or_default();

    format!("{}://{}{}{}{}", scheme, host, port_str, path, query)
}
