use slither_core::{CrawlScope, Seed, SeedConfig, SeedPriority};
use tracing::{info, warn};

const TRANCO_URL: &str = "https://tranco-list.eu/top-1m.csv.zip";
const HN_TOP_STORIES: &str = "https://hacker-news.firebaseio.com/v0/topstories.json";
const HN_BEST_STORIES: &str = "https://hacker-news.firebaseio.com/v0/beststories.json";
const HN_ITEM_URL: &str = "https://hacker-news.firebaseio.com/v0/item";

const JUNK_TLDS: &[&str] = &[
    ".cn", ".ru", ".su", ".ir", ".kp",
];

const JUNK_DOMAINS: &[&str] = &[
    "google.com", "googleapis.com", "gstatic.com", "googlevideo.com",
    "googleusercontent.com", "google-analytics.com", "googletagmanager.com",
    "googleadservices.com", "googlesyndication.com", "doubleclick.net",
    "facebook.com", "fbcdn.net", "fb.com", "instagram.com",
    "twitter.com", "x.com", "twimg.com", "t.co",
    "apple.com", "icloud.com", "mzstatic.com",
    "microsoft.com", "office.com", "live.com", "outlook.com",
    "bing.com", "msn.com", "windows.net", "azure.com",
    "amazonaws.com", "aws.amazon.com", "cloudfront.net",
    "cloudflare.com", "cdnjs.cloudflare.com",
    "akamai.net", "akamaihd.net", "akamaized.net",
    "fastly.net", "cloudflare-dns.com",
    "yahoo.com", "yimg.com",
    "baidu.com", "qq.com", "taobao.com", "tmall.com", "alipay.com",
    "weibo.com", "tencent.com", "jd.com", "163.com", "sohu.com",
    "mail.ru", "dzen.ru", "yandex.ru", "yandex.com", "vk.com",
    "tiktok.com", "bytedance.com",
    "whatsapp.com", "telegram.org",
    "zoom.us", "teams.microsoft.com",
    "gtld-servers.net", "root-servers.net",
    "w3.org", "schema.org", "creativecommons.org",
    "gravatar.com", "wp.com", "wordpress.org",
    "shopify.com", "squarespace.com", "wix.com",
    "stripe.com", "paypal.com",
    "recaptcha.net", "hcaptcha.com",
    "sentry.io", "bugsnag.com",
    "fontawesome.com", "fonts.googleapis.com",
    "jquery.com", "jsdelivr.net", "unpkg.com",
    "cookiebot.com", "onetrust.com", "cookielaw.org",
    "intercom.io", "zendesk.com", "freshdesk.com",
    "hotjar.com", "mixpanel.com", "segment.com", "amplitude.com",
    "pinterest.com", "snapchat.com",
    "spotify.com", "netflix.com", "twitch.tv", "discord.com",
    "linkedin.com",
    "amazon.com", "ebay.com", "aliexpress.com",
    "godaddy.com", "namecheap.com",
    "bit.ly", "tinyurl.com", "goo.gl",
    "archive.org",
];

const CONTENT_RICH_KEYWORDS: &[&str] = &[
    "blog", "docs", "dev", "wiki", "news", "tech", "science",
    "research", "engineering", "learn", "tutorial", "guide",
    "community", "forum", "discuss", "magazine", "journal",
    "review", "article", "write", "code", "data", "open",
    "edu", "university", "mit", "stanford", "berkeley",
];

const KNOWN_CONTENT_DOMAINS: &[&str] = &[
    "github.com", "github.io",
    "stackoverflow.com", "stackexchange.com", "superuser.com", "serverfault.com",
    "reddit.com",
    "wikipedia.org", "wikimedia.org", "wikidata.org",
    "medium.com", "substack.com", "dev.to", "hashnode.dev",
    "arxiv.org", "scholar.google.com",
    "docs.rs", "crates.io", "rust-lang.org",
    "python.org", "pypi.org", "readthedocs.io", "readthedocs.org",
    "mozilla.org", "developer.mozilla.org",
    "npmjs.com", "nodejs.org",
    "go.dev", "golang.org",
    "kubernetes.io", "docker.com",
    "news.ycombinator.com", "lobste.rs",
    "producthunt.com",
    "techcrunch.com", "arstechnica.com", "theverge.com", "wired.com",
    "bbc.com", "reuters.com", "nytimes.com", "washingtonpost.com",
    "nature.com", "sciencedirect.com", "ieee.org", "acm.org",
    "freecodecamp.org", "codecademy.com",
    "vercel.com", "netlify.com", "heroku.com",
    "digitalocean.com", "linode.com", "vultr.com",
    "huggingface.co", "openai.com", "anthropic.com",
    "gitlab.com", "bitbucket.org",
    "hackernoon.com", "infoq.com", "dzone.com",
    "smashingmagazine.com", "css-tricks.com", "alistapart.com",
    "martinfowler.com", "joelonsoftware.com",
];

fn is_junk_domain(domain: &str) -> bool {
    if JUNK_DOMAINS.iter().any(|j| domain == *j || domain.ends_with(&format!(".{j}"))) {
        return true;
    }
    if JUNK_TLDS.iter().any(|tld| domain.ends_with(tld)) {
        return true;
    }
    if domain.contains("cdn") || domain.contains("static") || domain.contains("cache")
        || domain.contains("proxy") || domain.contains("tracker") || domain.contains("analytics")
        || domain.contains("adserver") || domain.contains("pixel")
    {
        return true;
    }
    false
}

fn score_domain(domain: &str, rank: usize) -> f64 {
    let mut score: f64 = 1_000_000.0 / (rank as f64 + 100.0);

    if KNOWN_CONTENT_DOMAINS.iter().any(|k| domain == *k || domain.ends_with(&format!(".{k}"))) {
        score *= 5.0;
    }

    if CONTENT_RICH_KEYWORDS.iter().any(|kw| domain.contains(kw)) {
        score *= 2.0;
    }

    if domain.ends_with(".edu") || domain.ends_with(".ac.uk") || domain.ends_with(".edu.au") {
        score *= 3.0;
    }

    if domain.ends_with(".gov") {
        score *= 1.5;
    }

    if domain.ends_with(".io") || domain.ends_with(".dev") || domain.ends_with(".sh") {
        score *= 1.3;
    }

    score
}

fn depth_for_domain(domain: &str) -> usize {
    if KNOWN_CONTENT_DOMAINS.iter().any(|k| domain == *k || domain.ends_with(&format!(".{k}"))) {
        return 3;
    }
    if domain.contains("docs") || domain.contains("wiki") || domain.contains("dev")
        || domain.contains("blog") || domain.contains("learn")
    {
        return 3;
    }
    if domain.ends_with(".edu") || domain.ends_with(".ac.uk") {
        return 3;
    }
    2
}

fn scope_for_domain(domain: &str) -> CrawlScope {
    if domain.contains("news") || domain == "reddit.com" || domain == "lobste.rs"
        || domain.contains("ycombinator")
    {
        return CrawlScope::Any;
    }
    CrawlScope::SameDomain
}

fn priority_for_rank(rank: usize) -> SeedPriority {
    if rank <= 500 {
        SeedPriority::High
    } else if rank <= 5000 {
        SeedPriority::Normal
    } else {
        SeedPriority::Low
    }
}

pub async fn discover_from_tranco(existing: &[Seed], limit: usize) -> Vec<Seed> {
    info!("fetching Tranco top 1M list...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let zip_bytes = match client.get(TRANCO_URL).send().await {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                warn!("failed to download Tranco list: {e}");
                return Vec::new();
            }
        },
        Ok(r) => {
            warn!("Tranco download failed with status {}", r.status());
            return Vec::new();
        }
        Err(e) => {
            warn!("Tranco download error: {e}");
            return Vec::new();
        }
    };

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => {
            warn!("failed to open Tranco zip: {e}");
            return Vec::new();
        }
    };

    let csv_content = {
        let mut file = match archive.by_index(0) {
            Ok(f) => f,
            Err(e) => {
                warn!("failed to read Tranco zip entry: {e}");
                return Vec::new();
            }
        };
        let mut buf = String::new();
        use std::io::Read;
        if let Err(e) = file.read_to_string(&mut buf) {
            warn!("failed to read Tranco CSV: {e}");
            return Vec::new();
        }
        buf
    };

    let existing_domains: std::collections::HashSet<String> = existing
        .iter()
        .filter_map(|s| {
            url::Url::parse(s.url()).ok()
                .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
        })
        .collect();

    let mut candidates: Vec<(String, usize, f64)> = Vec::new();

    for line in csv_content.lines() {
        let parts: Vec<&str> = line.splitn(2, ',').collect();
        if parts.len() != 2 { continue; }
        let rank: usize = match parts[0].parse() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let domain = parts[1].trim().to_lowercase();

        if is_junk_domain(&domain) { continue; }
        if existing_domains.contains(&domain) { continue; }
        if existing_domains.contains(&format!("www.{domain}")) { continue; }

        let score = score_domain(&domain, rank);
        candidates.push((domain, rank, score));
    }

    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let picked: Vec<Seed> = candidates.iter()
        .take(limit)
        .map(|(domain, rank, _score)| {
            Seed::Configured(SeedConfig {
                url: format!("https://{domain}"),
                depth: depth_for_domain(domain),
                priority: priority_for_rank(*rank),
                scope: scope_for_domain(domain),
                sitemap: depth_for_domain(domain) >= 3,
            })
        })
        .collect();

    info!("Tranco: {} candidates after filtering, picked top {}", candidates.len(), picked.len());
    picked
}

pub async fn discover_from_hn(existing: &[Seed], limit: usize) -> Vec<Seed> {
    info!("fetching Hacker News top + best stories...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let mut story_ids: Vec<u64> = Vec::new();

    for endpoint in [HN_TOP_STORIES, HN_BEST_STORIES] {
        match client.get(endpoint).send().await {
            Ok(r) if r.status().is_success() => {
                if let Ok(ids) = r.json::<Vec<u64>>().await {
                    story_ids.extend(ids);
                }
            }
            _ => {
                warn!("failed to fetch HN stories from {endpoint}");
            }
        }
    }

    story_ids.sort_unstable();
    story_ids.dedup();

    let existing_urls: std::collections::HashSet<String> = existing
        .iter()
        .map(|s| s.url().to_string())
        .collect();

    let existing_domains: std::collections::HashSet<String> = existing
        .iter()
        .filter_map(|s| {
            url::Url::parse(s.url()).ok()
                .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
        })
        .collect();

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(10));
    let mut handles = Vec::new();

    for id in story_ids.into_iter().take(500) {
        let client = client.clone();
        let sem = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            let url = format!("{HN_ITEM_URL}/{id}.json");
            let resp = client.get(&url).send().await.ok()?;
            let item: serde_json::Value = resp.json().await.ok()?;
            item.get("url")?.as_str().map(|s| s.to_string())
        }));
    }

    let mut domain_counts: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for handle in handles {
        if let Ok(Some(story_url)) = handle.await {
            if let Ok(parsed) = url::Url::parse(&story_url) {
                if let Some(host) = parsed.host_str() {
                    let domain = host.trim_start_matches("www.").to_string();
                    if !existing_domains.contains(&domain)
                        && !existing_urls.contains(&story_url)
                        && !is_junk_domain(&domain)
                    {
                        domain_counts.entry(domain).or_default().push(story_url);
                    }
                }
            }
        }
    }

    let mut ranked: Vec<(String, Vec<String>)> = domain_counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let picked: Vec<Seed> = ranked.iter()
        .take(limit)
        .map(|(domain, urls)| {
            let best_url = urls.first().cloned().unwrap_or_else(|| format!("https://{domain}"));
            Seed::Configured(SeedConfig {
                url: best_url,
                depth: 2,
                priority: SeedPriority::Normal,
                scope: CrawlScope::Any,
                sitemap: false,
            })
        })
        .collect();

    info!("HN: found {} unique domains from stories, picked top {}", ranked.len(), picked.len());
    picked
}

pub async fn run_discovery(
    existing: &[Seed],
    tranco_limit: usize,
    hn_limit: usize,
    auto_add: bool,
    data_dir: &str,
) -> Vec<Seed> {
    let (tranco_seeds, hn_seeds) = tokio::join!(
        discover_from_tranco(existing, tranco_limit),
        discover_from_hn(existing, hn_limit),
    );

    let mut all_new: Vec<Seed> = Vec::new();
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

    for seed in tranco_seeds.into_iter().chain(hn_seeds.into_iter()) {
        let url = seed.url().to_string();
        if seen_urls.insert(url) {
            all_new.push(seed);
        }
    }

    println!("Discovered {} new seed(s)", all_new.len());

    if all_new.is_empty() {
        return all_new;
    }

    for seed in &all_new {
        match seed {
            Seed::Configured(c) => {
                println!("  + {} (depth={}, scope={:?}, priority={:?}{})",
                    c.url, c.depth, c.scope, c.priority,
                    if c.sitemap { ", sitemap" } else { "" }
                );
            }
            Seed::Simple(url) => {
                println!("  + {url}");
            }
        }
    }

    if auto_add {
        let mut merged = existing.to_vec();
        merged.extend(all_new.clone());
        match crate::seeds::save_seeds(data_dir, &merged) {
            Ok(()) => println!("Saved {} total seeds to seeds.json", merged.len()),
            Err(e) => eprintln!("Failed to save seeds: {e}"),
        }
    } else {
        println!("\nRun with --auto to add these to seeds.json");
    }

    all_new
}
