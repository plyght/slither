use crossbeam_deque::{Injector, Steal};
use dashmap::DashMap;
use slither_core::CrawlScope;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use xxhash_rust::xxh3::xxh3_64;

#[derive(Debug, Clone)]
pub struct CrawlTask {
    pub url: String,
    pub depth: usize,
    pub max_depth: usize,
    pub scope: CrawlScope,
    pub scope_domain: Option<String>,
}

pub struct Frontier {
    pub global: Injector<CrawlTask>,
    seen: DashMap<u64, ()>,
    pending: AtomicI64,
    pub notify: Arc<Notify>,
}

impl Frontier {
    pub fn new() -> Self {
        Self {
            global: Injector::new(),
            seen: DashMap::new(),
            pending: AtomicI64::new(0),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn with_known_urls(known: &[u64]) -> Self {
        let seen = DashMap::with_capacity(known.len());
        for &hash in known {
            seen.insert(hash, ());
        }
        tracing::info!(
            "frontier pre-loaded {} known URLs — will skip already-indexed pages",
            known.len()
        );
        Self {
            global: Injector::new(),
            seen,
            pending: AtomicI64::new(0),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn url_key(url: &str) -> u64 {
        xxh3_64(url.as_bytes())
    }

    pub fn seed(
        &self,
        url: String,
        max_depth: usize,
        scope: CrawlScope,
        scope_domain: Option<String>,
    ) {
        let key = Self::url_key(&url);
        self.seen.insert(key, ());
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.global.push(CrawlTask {
            url,
            depth: 0,
            max_depth,
            scope,
            scope_domain,
        });
        self.notify.notify_waiters();
    }

    pub fn try_push(
        &self,
        url: String,
        depth: usize,
        max_depth: usize,
        scope: CrawlScope,
        scope_domain: Option<String>,
        local: &Injector<CrawlTask>,
    ) -> bool {
        let key = Self::url_key(&url);
        if self.seen.insert(key, ()).is_none() {
            self.pending.fetch_add(1, Ordering::SeqCst);
            local.push(CrawlTask {
                url,
                depth,
                max_depth,
                scope,
                scope_domain,
            });
            self.notify.notify_waiters();
            true
        } else {
            false
        }
    }

    pub fn complete(&self) {
        self.pending.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn is_done(&self) -> bool {
        self.pending.load(Ordering::SeqCst) <= 0
    }

    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }
}

impl Default for Frontier {
    fn default() -> Self {
        Self::new()
    }
}

pub fn steal_task(
    local: &Injector<CrawlTask>,
    global: &Injector<CrawlTask>,
    peers: &[Arc<Injector<CrawlTask>>],
) -> Option<CrawlTask> {
    loop {
        match local.steal() {
            Steal::Success(t) => return Some(t),
            Steal::Retry => continue,
            Steal::Empty => break,
        }
    }

    loop {
        match global.steal() {
            Steal::Success(t) => return Some(t),
            Steal::Retry => continue,
            Steal::Empty => break,
        }
    }

    for peer in peers {
        loop {
            match peer.steal() {
                Steal::Success(t) => return Some(t),
                Steal::Retry => continue,
                Steal::Empty => break,
            }
        }
    }

    None
}
