use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fang::{Fang, FangConfig};
use iris::Iris;
use slither_core::{
    CrawlScope, RawPage, SearchMode, SearchQuery, Seed, SlitherConfig, SlitherResult,
};
use snake::Crawler;
use tome::Index;
use tracing::{debug, info, warn};
use venom::Ranker;

pub async fn crawl(config: SlitherConfig, seeds: Vec<Seed>) -> SlitherResult<()> {
    let seed_count = seeds.len();
    println!(
        "Starting crawl from {} seed(s) (concurrent={})...",
        seed_count, config.crawler.max_concurrent
    );

    std::fs::create_dir_all(&config.index.data_dir)?;
    std::fs::create_dir_all(&config.embedder.data_dir)?;

    let transformer = Fang::new(FangConfig::default());
    let index = Index::open_for_crawl(Path::new(&config.index.data_dir))?;
    let embedder = Iris::new(config.embedder.clone())?;
    let known_ids = index.known_doc_ids();
    let mut ranker = Ranker::new(index, embedder);

    let favicon_dir = PathBuf::from(&config.data_dir).join("favicons");
    std::fs::create_dir_all(&favicon_dir)?;

    let snake = Crawler::new(config.crawler.clone()).with_known_urls(known_ids);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<RawPage>(2048);

    let mut sorted_seeds = seeds;
    sorted_seeds.sort_by_key(|b| std::cmp::Reverse(b.priority()));

    let mut resolved: Vec<(String, usize, CrawlScope)> = Vec::new();
    for seed in &sorted_seeds {
        if seed.sitemap() {
            let sitemap_urls = snake::sitemap::discover_sitemap_urls(seed.url(), 500).await;
            for url in sitemap_urls {
                resolved.push((url, seed.depth(), seed.scope()));
            }
        }
        resolved.push((seed.url().to_string(), seed.depth(), seed.scope()));
    }

    let crawl_handle = tokio::spawn(async move { snake.crawl(resolved, tx).await });

    let mut pages_crawled: usize = 0;
    let mut pages_indexed: usize = 0;
    let mut domain_favicons: HashMap<String, Option<String>> = HashMap::new();

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");

    const BATCH_SIZE: usize = 32;
    let batch_timeout = std::time::Duration::from_millis(100);
    let mut buffer: Vec<RawPage> = Vec::with_capacity(BATCH_SIZE);
    let mut done = false;

    loop {
        let pages_before_fill = pages_crawled;

        while !done && buffer.len() < BATCH_SIZE {
            tokio::select! {
                biased;
                _ = &mut ctrl_c => {
                    println!("\nInterrupted (SIGINT) — flushing index...");
                    done = true;
                }
                _ = sigterm.recv() => {
                    println!("\nTerminated (SIGTERM) — flushing index...");
                    done = true;
                }
                msg = rx.recv() => {
                    match msg {
                        None => { done = true; }
                        Some(raw_page) => {
                            pages_crawled += 1;
                            buffer.push(raw_page);
                        }
                    }
                }
                _ = tokio::time::sleep(batch_timeout), if !buffer.is_empty() => {
                    break;
                }
            }
        }

        if !buffer.is_empty() {
            let batch = std::mem::replace(&mut buffer, Vec::with_capacity(BATCH_SIZE));
            let transformer_clone = transformer.clone();

            let transform_results = tokio::task::spawn_blocking(move || {
                batch
                    .into_iter()
                    .map(|raw_page| {
                        let url = raw_page.url.clone();
                        let domain = raw_page.domain.clone();
                        match transformer_clone.transform(&raw_page) {
                            Ok(doc) => {
                                let quality = fang::content_quality_score(&doc.body);
                                Some((doc, quality, url, domain))
                            }
                            Err(e) => {
                                tracing::warn!("transform error for {url}: {e}");
                                None
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .await;

            if let Ok(results) = transform_results {
                let mut docs_to_index = Vec::new();
                for (doc, quality, page_url, page_domain) in results.into_iter().flatten() {
                    let is_homepage = is_homepage_url(&page_url);
                    if quality < 0.15 && !is_homepage {
                        debug!("skipping low-quality page ({quality:.2}): {}", page_url);
                        continue;
                    }
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        domain_favicons.entry(page_domain)
                    {
                        e.insert(doc.favicon_url.clone());
                    }
                    docs_to_index.push(doc);
                }

                let batch_count = docs_to_index.len();
                if !docs_to_index.is_empty() {
                    match ranker.index_documents_batch(&docs_to_index) {
                        Ok(()) => pages_indexed += batch_count,
                        Err(e) => {
                            warn!("batch index error: {e}");
                        }
                    }
                }
            }

            if (pages_crawled / 100) > (pages_before_fill / 100) {
                let (can_continue, status) = check_storage_limits(
                    &config.index.data_dir,
                    &config.embedder.data_dir,
                    &config.storage,
                );
                println!(
                    "  Crawled {} pages, indexed {} documents | {}",
                    pages_crawled, pages_indexed, status
                );
                if !can_continue {
                    println!("\nStopping crawl: {}", status);
                    break;
                }
            }
        }

        if done && buffer.is_empty() {
            break;
        }
    }

    drop(rx);
    crawl_handle.abort();
    let _ = crawl_handle.await;
    ranker.flush()?;

    println!("Crawl complete.");
    println!(
        "  Crawled {} pages, indexed {} documents",
        pages_crawled, pages_indexed
    );

    let new_domains: Vec<(String, String)> = domain_favicons
        .into_iter()
        .filter_map(|(domain, url)| {
            let url = url?;
            let path = favicon_dir.join(sanitize_domain(&domain));
            if path.exists() {
                return None;
            }
            Some((domain, url))
        })
        .collect();

    if !new_domains.is_empty() {
        println!(
            "Fetching favicons for {} new domain(s)...",
            new_domains.len()
        );
        fetch_favicons(&favicon_dir, &new_domains).await;
    }

    info!(pages_crawled, pages_indexed, "crawl pipeline finished");

    Ok(())
}

pub async fn search(
    config: SlitherConfig,
    query: SearchQuery,
    mode: SearchMode,
) -> SlitherResult<()> {
    let index = Index::open(Path::new(&config.index.data_dir))?;
    let embedder = Iris::new(config.embedder.clone())?;
    let mut ranker = Ranker::new(index, embedder);

    let results = ranker.search(&query, mode)?;

    if results.is_empty() {
        println!("No results found for \"{}\".", query.text);
        return Ok(());
    }

    println!();
    for (i, result) in results.iter().enumerate() {
        let rank = i + 1;
        let score = result.score;
        let title = if result.title.is_empty() {
            "(untitled)"
        } else {
            result.title.as_str()
        };

        println!("[{rank}] ({score:.3}) {title}");
        println!("    {}", result.url);

        if !result.snippet.is_empty() {
            let snippet = truncate_snippet(&result.snippet, 120);
            println!("    ...{snippet}...");
        }

        println!();
    }

    info!(
        mode = ?mode,
        query = query.text,
        count = results.len(),
        "search complete"
    );

    Ok(())
}

pub async fn stats(config: SlitherConfig) -> SlitherResult<()> {
    let index_dir = &config.index.data_dir;
    let vector_dir = &config.embedder.data_dir;

    let index = Index::open(Path::new(index_dir))?;
    let doc_count = index.doc_count();
    let index_size = dir_size(index_dir);
    let vector_size = dir_size(vector_dir);

    println!("Slither Index Statistics");
    println!("  Index directory  : {index_dir}");
    println!("  Vector directory : {vector_dir}");
    println!("  Documents        : {doc_count}");
    println!("  Index size       : {}", human_bytes(index_size));
    println!("  Vector size      : {}", human_bytes(vector_size));

    Ok(())
}

pub async fn config_cmd(config: SlitherConfig, show: bool, init: bool) -> SlitherResult<()> {
    if show {
        let json = serde_json::to_string_pretty(&config)?;
        println!("{json}");
        return Ok(());
    }

    if init {
        let default_cfg = SlitherConfig::default();
        let json = serde_json::to_string_pretty(&default_cfg)?;
        std::fs::write("slither.json", json)?;
        println!("Default config written to slither.json");
        return Ok(());
    }

    eprintln!("Use --show to print config or --init to write defaults to slither.json");
    Ok(())
}

fn truncate_snippet(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}\u{2026}")
}

fn dir_size(path: &str) -> u64 {
    use std::fs;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn sanitize_domain(domain: &str) -> String {
    domain
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn fetch_favicons(favicon_dir: &Path, domains: &[(String, String)]) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .unwrap_or_default();

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(10));
    let mut handles = Vec::new();

    for (domain, url) in domains {
        let client = client.clone();
        let url = url.clone();
        let domain = domain.clone();
        let dir = favicon_dir.to_path_buf();
        let sem = semaphore.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            let resp = client.get(&url).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let bytes = resp.bytes().await.ok()?;
            if bytes.is_empty() || bytes.len() > 102_400 {
                return None;
            }
            let path = dir.join(sanitize_domain(&domain));
            std::fs::write(&path, &bytes).ok()?;
            Some(domain)
        }));
    }

    let mut fetched = 0usize;
    for handle in handles {
        if let Ok(Some(_)) = handle.await {
            fetched += 1;
        }
    }
    println!("  Fetched {fetched} favicon(s)");
}

fn is_homepage_url(url: &str) -> bool {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let path = match without_scheme.find('/') {
        Some(i) => &without_scheme[i..],
        None => "",
    };
    let path_clean = path.split('?').next().unwrap_or(path);
    let path_clean = path_clean.split('#').next().unwrap_or(path_clean);
    path_clean.is_empty()
        || path_clean == "/"
        || path_clean == "/index.html"
        || path_clean == "/index.htm"
}

#[cfg(unix)]
fn check_storage_limits(
    index_dir: &str,
    vector_dir: &str,
    config: &slither_core::StorageConfig,
) -> (bool, String) {
    let index_size = dir_size(index_dir);
    let vector_size = dir_size(vector_dir);
    let total_size = index_size + vector_size;

    const GB: u64 = 1024 * 1024 * 1024;
    let total_gb = total_size as f64 / GB as f64;

    if let Some(max_gb) = config.max_gb {
        if total_gb >= max_gb {
            return (
                false,
                format!("Storage limit reached: {:.2}GB / {:.0}GB", total_gb, max_gb),
            );
        }
    }

    if total_gb >= config.warning_threshold_gb {
        return (
            true,
            format!("WARNING: {:.2}GB (limit: {:?}GB)", total_gb, config.max_gb),
        );
    }

    (true, format!("{:.2}GB", total_gb))
}
