pub mod fetcher;
pub mod frontier;
pub mod rate_limiter;
pub mod robots;
pub mod worker;

pub use slither_core::{CrawlerConfig, RawPage};

use crossbeam_deque::Injector;
use fetcher::Fetcher;
use frontier::Frontier;
use rate_limiter::DomainRateLimiter;
use robots::RobotsCache;
use slither_core::SlitherError;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::info;
use url::Url;

pub struct Crawler {
    config: CrawlerConfig,
}

impl Crawler {
    pub fn new(config: CrawlerConfig) -> Self {
        Self { config }
    }

    pub async fn crawl(
        &self,
        seeds: Vec<String>,
        tx: Sender<RawPage>,
    ) -> Result<(), SlitherError> {
        let config = &self.config;
        let n_workers = config.max_concurrent.max(1);

        let fetcher = Arc::new(Fetcher::new(&config.user_agent, config.request_timeout_secs)?);
        let robots = Arc::new(RobotsCache::new(fetcher.client.clone(), config.user_agent.clone()));
        let rate_limiter = Arc::new(DomainRateLimiter::new(config.rate_limit_per_second)?);
        let frontier = Arc::new(Frontier::new());

        let mut valid_seeds = 0usize;
        for raw in &seeds {
            match normalize_url(raw) {
                Some(url) => {
                    frontier.seed(url);
                    valid_seeds += 1;
                }
                None => {
                    tracing::warn!("ignoring invalid seed URL: {}", raw);
                }
            }
        }

        if valid_seeds == 0 {
            info!("no valid seed URLs — crawl aborted");
            return Ok(());
        }

        info!(
            "starting crawl: {} seeds, {} workers, max_depth={}",
            valid_seeds, n_workers, config.max_depth
        );

        let worker_locals: Vec<Arc<Injector<frontier::CrawlTask>>> =
            (0..n_workers).map(|_| Arc::new(Injector::new())).collect();

        let mut handles = Vec::with_capacity(n_workers);

        for id in 0..n_workers {
            let peers: Vec<Arc<Injector<frontier::CrawlTask>>> = worker_locals
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != id)
                .map(|(_, q)| q.clone())
                .collect();

            let ctx = worker::WorkerContext {
                id,
                local: worker_locals[id].clone(),
                peers,
                frontier: frontier.clone(),
                fetcher: fetcher.clone(),
                robots: robots.clone(),
                rate_limiter: rate_limiter.clone(),
                config: config.clone(),
                tx: tx.clone(),
            };

            handles.push(tokio::spawn(worker::run(ctx)));
        }

        for handle in handles {
            handle
                .await
                .map_err(|e| SlitherError::Crawl(format!("worker panicked: {}", e)))?;
        }

        info!("crawl complete");
        Ok(())
    }
}

fn normalize_url(raw: &str) -> Option<String> {
    let mut parsed = Url::parse(raw).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    parsed.set_fragment(None);
    Some(parsed.to_string())
}
