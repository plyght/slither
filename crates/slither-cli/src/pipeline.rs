use std::path::Path;

use fang::{Fang, FangConfig};
use iris::Iris;
use slither_core::{RawPage, SearchMode, SearchQuery, SlitherConfig, SlitherResult};
use snake::Crawler;
use tome::Index;
use tracing::{info, warn};
use venom::Ranker;

pub async fn crawl(config: SlitherConfig, urls: Vec<String>) -> SlitherResult<()> {
    let seed_count = urls.len();
    println!(
        "Starting crawl from {} seed URL(s) (depth={}, concurrent={})...",
        seed_count, config.crawler.max_depth, config.crawler.max_concurrent
    );

    std::fs::create_dir_all(&config.index.data_dir)?;
    std::fs::create_dir_all(&config.embedder.data_dir)?;

    let transformer = Fang::new(FangConfig::default());
    let index = Index::open(Path::new(&config.index.data_dir))?;
    let embedder = Iris::new(config.embedder.clone())?;
    let known_ids = index.known_doc_ids();
    let mut ranker = Ranker::new(index, embedder);

    let snake = Crawler::new(config.crawler.clone()).with_known_urls(known_ids);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<RawPage>(256);

    let crawl_handle = tokio::spawn(async move { snake.crawl(urls, tx).await });

    let mut pages_crawled: usize = 0;
    let mut pages_indexed: usize = 0;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                println!("\nInterrupted (SIGINT) — flushing index...");
                break;
            }
            _ = sigterm.recv() => {
                println!("\nTerminated (SIGTERM) — flushing index...");
                break;
            }
            msg = rx.recv() => {
                match msg {
                    None => break,
                    Some(raw_page) => {
                        pages_crawled += 1;
                        match transformer.transform(&raw_page) {
                            Ok(doc) => {
                                match ranker.index_document(&doc) {
                                    Ok(()) => pages_indexed += 1,
                                    Err(e) => {
                                        warn!("index error for {}: {e}", raw_page.url);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("transform error for {}: {e}", raw_page.url);
                            }
                        }

                        if pages_crawled % 100 == 0 {
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
                }
            }
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
    let ranker = Ranker::new(index, embedder);

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

#[cfg(unix)]
fn check_storage_limits(index_dir: &str, vector_dir: &str, config: &slither_core::StorageConfig) -> (bool, String) {
    let index_size = dir_size(index_dir);
    let vector_size = dir_size(vector_dir);
    let total_size = index_size + vector_size;
    
    const GB: u64 = 1024 * 1024 * 1024;
    let total_gb = total_size as f64 / GB as f64;
    
    if let Some(max_gb) = config.max_gb {
        if total_gb >= max_gb {
            return (false, format!("Storage limit reached: {:.2}GB / {:.0}GB", total_gb, max_gb));
        }
    }
    
    if total_gb >= config.warning_threshold_gb {
        return (true, format!("WARNING: {:.2}GB (limit: {:?}GB)", total_gb, config.max_gb));
    }
    
    (true, format!("{:.2}GB", total_gb))
}
