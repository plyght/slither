use clap::{Parser, Subcommand, ValueEnum};
use slither_core::{SearchMode, SearchQuery, Seed, SlitherConfig};
use tracing::error;

mod admin;
mod api;
mod discover;
mod pipeline;
mod seeds;

#[derive(Parser)]
#[command(
    name = "slither",
    version,
    about = "A fast, hybrid web search engine",
    long_about = None
)]
struct Cli {
    #[arg(long, short, global = true, help = "Enable verbose logging")]
    verbose: bool,

    #[arg(long, global = true, value_name = "PATH", help = "Path to config file")]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Crawl URLs and build the search index")]
    Crawl {
        #[arg(value_name = "URL", help = "Seed URL(s) to crawl")]
        urls: Vec<String>,

        #[arg(long, help = "Load seeds from seeds.json instead of CLI args")]
        seeds: bool,

        #[arg(long, default_value = "3", help = "Maximum crawl depth")]
        depth: usize,

        #[arg(long, default_value = "50", help = "Number of concurrent workers")]
        concurrent: usize,

        #[arg(long, help = "Output data directory (overrides config)")]
        output_dir: Option<String>,

        #[arg(long, help = "Max storage in GB (e.g. 100)")]
        max_storage_gb: Option<f64>,

        #[arg(long, default_value = "10", help = "Warning threshold in GB")]
        warning_threshold_gb: f64,
    },

    #[command(about = "Search the index")]
    Search {
        #[arg(value_name = "QUERY", help = "Search query")]
        query: String,

        #[arg(
            long,
            short,
            default_value = "10",
            help = "Number of results to return"
        )]
        limit: usize,

        #[arg(
            long,
            short,
            value_enum,
            default_value = "hybrid",
            help = "Search mode"
        )]
        mode: CliSearchMode,

        #[arg(long, help = "Data directory (overrides config)")]
        output_dir: Option<String>,
    },

    #[command(about = "Show index statistics")]
    Stats {
        #[arg(long, help = "Data directory (overrides config)")]
        output_dir: Option<String>,
    },

    #[command(about = "Start HTTP API server")]
    Serve {
        #[arg(long, default_value = "8080", help = "Port to listen on")]
        port: u16,

        #[arg(long, default_value = "0.0.0.0", help = "Address to bind to")]
        host: String,

        #[arg(long, help = "Data directory (overrides config)")]
        output_dir: Option<String>,
    },

    #[command(about = "Manage configuration")]
    Config {
        #[arg(long, help = "Print current configuration as JSON")]
        show: bool,

        #[arg(long, help = "Write default configuration to slither.json")]
        init: bool,
    },

    #[command(about = "Discover new seed URLs from Tranco top 1M and Hacker News")]
    Discover {
        #[arg(long, default_value = "200", help = "Max seeds from Tranco list")]
        tranco_limit: usize,

        #[arg(long, default_value = "50", help = "Max seeds from HN stories")]
        hn_limit: usize,

        #[arg(long, help = "Auto-add discovered seeds to seeds.json")]
        auto: bool,

        #[arg(long, help = "Data directory (overrides config)")]
        output_dir: Option<String>,
    },

    #[command(alias = "a", about = "Manage the search engine remotely")]
    Admin {
        #[command(subcommand)]
        action: AdminAction,

        #[arg(
            long,
            short,
            env = "SLITHER_SERVER",
            default_value = "http://localhost:8080",
            help = "Server URL"
        )]
        server: String,

        #[arg(long, short, env = "SLITHER_ADMIN_KEY", help = "Admin API key")]
        key: String,
    },
}

#[derive(Subcommand)]
pub enum AdminAction {
    #[command(alias = "ls", about = "List seed URLs")]
    Seeds,
    #[command(alias = "add", about = "Add a seed URL")]
    AddSeed {
        url: String,
        #[arg(long, help = "Crawl depth for this seed")]
        depth: Option<usize>,
        #[arg(long, help = "Scope: any, same_domain, same_origin")]
        scope: Option<String>,
        #[arg(long, help = "Priority: low, normal, high")]
        priority: Option<String>,
        #[arg(long, help = "Discover URLs from sitemap.xml")]
        sitemap: bool,
    },
    #[command(alias = "rm", about = "Remove a seed URL")]
    RemoveSeed { url: String },
    #[command(alias = "c", about = "Trigger a crawl")]
    Crawl,
    #[command(alias = "st", about = "Check server status")]
    Status,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliSearchMode {
    Text,
    Semantic,
    Hybrid,
}

impl From<CliSearchMode> for SearchMode {
    fn from(m: CliSearchMode) -> Self {
        match m {
            CliSearchMode::Text => SearchMode::Text,
            CliSearchMode::Semantic => SearchMode::Semantic,
            CliSearchMode::Hybrid => SearchMode::Hybrid,
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let mut config = load_config(cli.config.as_deref());

    let result = match cli.command {
        Commands::Crawl {
            urls,
            seeds: use_seeds_file,
            depth,
            concurrent,
            output_dir,
            max_storage_gb,
            warning_threshold_gb,
        } => {
            config.crawler.max_depth = depth;
            config.crawler.max_concurrent = concurrent;
            if let Some(dir) = output_dir {
                config.data_dir = dir.clone();
                config.index.data_dir = format!("{dir}/index");
                config.embedder.data_dir = format!("{dir}/vectors");
            }
            config.storage.max_gb = max_storage_gb;
            config.storage.warning_threshold_gb = warning_threshold_gb;
            let seed_list: Vec<Seed> = if use_seeds_file {
                seeds::load_seeds(&config.data_dir)
            } else if !urls.is_empty() {
                urls.into_iter().map(Seed::Simple).collect()
            } else {
                eprintln!("Error: provide seed URLs or use --seeds to load from seeds.json");
                std::process::exit(1);
            };
            pipeline::crawl(config, seed_list).await
        }

        Commands::Search {
            query,
            limit,
            mode,
            output_dir,
        } => {
            if let Some(dir) = output_dir {
                config.data_dir = dir.clone();
                config.index.data_dir = format!("{dir}/index");
                config.embedder.data_dir = format!("{dir}/vectors");
            }
            let sq = SearchQuery { text: query, limit };
            pipeline::search(config, sq, mode.into()).await
        }

        Commands::Stats { output_dir } => {
            if let Some(dir) = output_dir {
                config.data_dir = dir.clone();
                config.index.data_dir = format!("{dir}/index");
                config.embedder.data_dir = format!("{dir}/vectors");
            }
            pipeline::stats(config).await
        }

        Commands::Serve {
            port,
            host,
            output_dir,
        } => {
            if let Some(dir) = output_dir {
                config.data_dir = dir.clone();
                config.index.data_dir = format!("{dir}/index");
                config.embedder.data_dir = format!("{dir}/vectors");
            }
            api::serve(config, &host, port).await
        }

        Commands::Config { show, init } => pipeline::config_cmd(config, show, init).await,

        Commands::Discover {
            tranco_limit,
            hn_limit,
            auto,
            output_dir,
        } => {
            if let Some(dir) = output_dir {
                config.data_dir = dir.clone();
                config.index.data_dir = format!("{dir}/index");
                config.embedder.data_dir = format!("{dir}/vectors");
            }
            let existing = seeds::load_seeds(&config.data_dir);
            discover::run_discovery(&existing, tranco_limit, hn_limit, auto, &config.data_dir)
                .await;
            Ok(())
        }

        Commands::Admin {
            action,
            server,
            key,
        } => {
            admin::run_admin(action, server, key).await;
            return;
        }
    };

    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

fn init_tracing(verbose: bool) {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };

    fmt().with_env_filter(filter).with_target(false).init();
}

fn load_config(path: Option<&str>) -> SlitherConfig {
    let candidates = [
        path.map(|p| p.to_string()),
        Some("slither.json".to_string()),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            match serde_json::from_str::<SlitherConfig>(&contents) {
                Ok(cfg) => return cfg,
                Err(e) => {
                    eprintln!("Warning: failed to parse config at {candidate}: {e}");
                }
            }
        }
    }

    SlitherConfig::default()
}
