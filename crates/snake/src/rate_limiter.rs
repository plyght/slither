use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use slither_core::SlitherError;
use std::num::NonZeroU32;
use std::sync::Arc;

pub struct DomainRateLimiter {
    limiters: DashMap<String, Arc<DefaultDirectRateLimiter>>,
    quota: Quota,
}

impl DomainRateLimiter {
    pub fn new(rate_limit_per_second: u32) -> Result<Self, SlitherError> {
        let rate = rate_limit_per_second.max(1);
        let nz_rate = NonZeroU32::new(rate)
            .ok_or_else(|| SlitherError::Crawl("rate_limit_per_second must be non-zero".into()))?;
        let quota = Quota::per_second(nz_rate);
        Ok(Self {
            limiters: DashMap::new(),
            quota,
        })
    }

    pub async fn wait_for_domain(&self, domain: &str) {
        let quota = self.quota;
        let limiter = {
            let entry = self
                .limiters
                .entry(domain.to_string())
                .or_insert_with(|| Arc::new(RateLimiter::direct(quota)));
            entry.clone()
        };
        limiter.until_ready().await;
    }

    pub async fn wait_for_domain_with_delay(&self, domain: &str, crawl_delay_secs: Option<u64>) {
        if let Some(delay) = crawl_delay_secs {
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        } else {
            self.wait_for_domain(domain).await;
        }
    }
}
