use crossbeam_deque::Injector;
use regex::Regex;
use slither_core::CrawlerConfig;
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

async fn process_task(ctx: &WorkerContext, task: CrawlTask) {
    let url = &task.url;

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

    if task.depth < ctx.config.max_depth {
        let base = Url::parse(&final_url).unwrap_or(parsed_url);
        let links = extract_links(&html_clone, &base);
        for link in links {
            ctx.frontier.try_push(link, task.depth + 1, &ctx.local);
        }
    }

    ctx.frontier.complete();
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
