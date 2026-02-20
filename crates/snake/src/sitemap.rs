use regex::Regex;
use tracing::{debug, info};
use url::Url;

pub async fn discover_sitemap_urls(seed_url: &str, max_urls: usize) -> Vec<String> {
    let base = match Url::parse(seed_url) {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };

    let sitemap_url = format!(
        "{}://{}/sitemap.xml",
        base.scheme(),
        base.host_str().unwrap_or("")
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let resp = match client.get(&sitemap_url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            debug!("no sitemap found for {}", base.host_str().unwrap_or(""));
            return Vec::new();
        }
    };

    let body = match resp.text().await {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let mut urls = Vec::new();

    let sitemap_index_re = Regex::new(r"(?i)<sitemapindex").unwrap();
    if sitemap_index_re.is_match(&body) {
        let sub_loc_re = Regex::new(r"<loc>\s*(https?://[^<\s]+)\s*</loc>").unwrap();
        let sub_sitemaps: Vec<String> = sub_loc_re
            .captures_iter(&body)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        for sub_url in sub_sitemaps {
            if urls.len() >= max_urls {
                break;
            }
            if let Ok(r) = client.get(&sub_url).send().await {
                if r.status().is_success() {
                    if let Ok(sub_body) = r.text().await {
                        let remaining = max_urls - urls.len();
                        urls.extend(parse_sitemap_xml(&sub_body, remaining));
                    }
                }
            }
        }
    } else {
        urls = parse_sitemap_xml(&body, max_urls);
    }

    info!(
        "discovered {} URLs from sitemap for {}",
        urls.len(),
        base.host_str().unwrap_or("")
    );
    urls
}

fn parse_sitemap_xml(xml: &str, max_urls: usize) -> Vec<String> {
    let loc_re = Regex::new(r"<loc>\s*(https?://[^<\s]+)\s*</loc>").unwrap();
    loc_re
        .captures_iter(xml)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .take(max_urls)
        .collect()
}
