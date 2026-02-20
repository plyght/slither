use crossbeam_deque::Injector;
use regex::Regex;
use slither_core::{CrawlScope, CrawlerConfig};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::{debug, info, warn};
use url::Url;

use crate::{
    fetcher::Fetcher,
    frontier::{steal_task, CrawlTask, Frontier},
    rate_limiter::DomainRateLimiter,
    robots::RobotsCache,
};
use slither_core::RawPage;

pub struct WorkerContext {
    pub id: usize,
    pub local: Arc<Injector<CrawlTask>>,
    pub peers: Vec<Arc<Injector<CrawlTask>>>,
    pub frontier: Arc<Frontier>,
    pub fetcher: Arc<Fetcher>,
    pub robots: Arc<RobotsCache>,
    pub rate_limiter: Arc<DomainRateLimiter>,
    pub config: CrawlerConfig,
    pub tx: Sender<RawPage>,
}

pub async fn run(ctx: WorkerContext) {
    info!("worker {} started", ctx.id);

    loop {
        let task = steal_task(&ctx.local, &ctx.frontier.global, &ctx.peers);

        match task {
            Some(task) => {
                process_task(&ctx, task).await;
            }
            None => {
                if ctx.frontier.is_done() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
    }

    info!("worker {} finished", ctx.id);
}

const SKIP_EXTENSIONS: &[&str] = &[
    ".tar.gz",
    ".tgz",
    ".tar.bz2",
    ".tar.xz",
    ".gz",
    ".bz2",
    ".xz",
    ".zip",
    ".rar",
    ".7z",
    ".msi",
    ".exe",
    ".dmg",
    ".pkg",
    ".deb",
    ".rpm",
    ".appimage",
    ".iso",
    ".img",
    ".pdf",
    ".doc",
    ".docx",
    ".xls",
    ".xlsx",
    ".ppt",
    ".pptx",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".webp",
    ".svg",
    ".ico",
    ".bmp",
    ".tiff",
    ".mp3",
    ".mp4",
    ".avi",
    ".mkv",
    ".mov",
    ".flv",
    ".wmv",
    ".wav",
    ".ogg",
    ".webm",
    ".woff",
    ".woff2",
    ".ttf",
    ".eot",
    ".otf",
    ".css",
    ".js",
    ".mjs",
    ".map",
    ".wasm",
    ".bin",
    ".dat",
    ".o",
    ".so",
    ".dylib",
    ".dll",
    ".a",
    ".lib",
];

fn should_skip_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    SKIP_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

async fn process_task(ctx: &WorkerContext, task: CrawlTask) {
    let url = &task.url;

    if should_skip_url(url) {
        debug!("skipping non-HTML URL: {}", url);
        ctx.frontier.complete();
        return;
    }

    if ctx.config.respect_robots && !ctx.robots.is_allowed(url).await {
        debug!("robots.txt disallows {}", url);
        ctx.frontier.complete();
        return;
    }

    let parsed_url = match Url::parse(url) {
        Ok(u) => u,
        Err(e) => {
            warn!("invalid URL {}: {}", url, e);
            ctx.frontier.complete();
            return;
        }
    };

    let domain = parsed_url.host_str().unwrap_or("").to_string();
    ctx.rate_limiter.wait_for_domain(&domain).await;

    debug!("worker {} fetching depth={} {}", ctx.id, task.depth, url);

    let raw_page = match ctx.fetcher.fetch(url).await {
        Some(p) => p,
        None => {
            ctx.frontier.complete();
            return;
        }
    };

    let html_clone = raw_page.html.clone();
    let final_url = raw_page.url.clone();

    if ctx.tx.send(raw_page).await.is_err() {
        warn!("output channel closed, stopping worker {}", ctx.id);
        ctx.frontier.complete();
        return;
    }

    if task.depth < task.max_depth {
        let base = Url::parse(&final_url).unwrap_or(parsed_url);
        let links = extract_links(&html_clone, &base);
        for link in links {
            if !should_skip_url(&link) && matches_scope(&link, &task) {
                ctx.frontier.try_push(
                    link,
                    task.depth + 1,
                    task.max_depth,
                    task.scope,
                    task.scope_domain.clone(),
                    &ctx.local,
                );
            }
        }
    }

    ctx.frontier.complete();
}

fn matches_scope(link_url: &str, task: &CrawlTask) -> bool {
    match task.scope {
        CrawlScope::Any => true,
        CrawlScope::SameDomain => {
            let link_domain = Url::parse(link_url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            match (&task.scope_domain, link_domain) {
                (Some(seed_domain), Some(link_dom)) => {
                    extract_root_domain(&link_dom) == extract_root_domain(seed_domain)
                }
                _ => false,
            }
        }
        CrawlScope::SameOrigin => {
            let link_origin = Url::parse(link_url)
                .ok()
                .map(|u| u.origin().ascii_serialization());
            let seed_origin = task.scope_domain.as_ref().and_then(|d| {
                Url::parse(&format!("https://{d}"))
                    .ok()
                    .map(|u| u.origin().ascii_serialization())
            });
            link_origin == seed_origin
        }
    }
}

fn extract_root_domain(host: &str) -> &str {
    let parts: Vec<&str> = host.rsplit('.').collect();
    if parts.len() <= 2 {
        return host;
    }
    let sld = parts.get(1).unwrap_or(&"");
    if matches!(*sld, "co" | "com" | "org" | "net" | "gov" | "edu" | "ac") && parts.len() > 2 {
        let len = parts[0].len() + 1 + parts[1].len() + 1 + parts[2].len();
        &host[host.len() - len..]
    } else {
        let len = parts[0].len() + 1 + parts[1].len();
        &host[host.len() - len..]
    }
}

fn extract_links(html: &str, base: &Url) -> Vec<String> {
    let re = Regex::new(r#"(?i)href\s*=\s*["']([^"']+)["']"#).unwrap();
    let mut links = Vec::new();

    for cap in re.captures_iter(html) {
        let href = match cap.get(1) {
            Some(m) => m.as_str(),
            None => continue,
        };

        let resolved = match base.join(href) {
            Ok(u) => u,
            Err(_) => continue,
        };

        if resolved.scheme() != "http" && resolved.scheme() != "https" {
            continue;
        }

        let mut normalized = resolved;
        normalized.set_fragment(None);

        links.push(normalized.to_string());
    }

    links
}
