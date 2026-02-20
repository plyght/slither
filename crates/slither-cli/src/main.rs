use clap::{Parser, Subcommand, ValueEnum};
use slither_core::{SearchMode, SearchQuery, SlitherConfig};
use tracing::error;

mod pipeline;
mod api;

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
        #[arg(value_name = "URL", required = true, help = "Seed URL(s) to crawl")]
        urls: Vec<String>,

        #[arg(long, default_value = "3", help = "Maximum crawl depth")]
        depth: usize,

        #[arg(long, default_value = "50", help = "Number of concurrent workers")]
        concurrent: usize,

        #[arg(long, default_value = "./slither_data", help = "Output data directory")]
        output_dir: String,

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

        #[arg(long, default_value = "./slither_data", help = "Data directory")]
        output_dir: String,
    },

    #[command(about = "Show index statistics")]
    Stats {
        #[arg(long, default_value = "./slither_data", help = "Data directory")]
        output_dir: String,
    },

    #[command(about = "Start HTTP API server")]
    Serve {
        #[arg(long, default_value = "8080", help = "Port to listen on")]
        port: u16,

        #[arg(long, default_value = "0.0.0.0", help = "Address to bind to")]
        host: String,

        #[arg(long, default_value = "./slither_data", help = "Data directory")]
        output_dir: String,
    },

    #[command(about = "Manage configuration")]
    Config {
        #[arg(long, help = "Print current configuration as JSON")]
        show: bool,

        #[arg(long, help = "Write default configuration to slither.json")]
        init: bool,
    },
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
            depth,
            concurrent,
            output_dir,
            max_storage_gb,
            warning_threshold_gb,
        } => {
            config.crawler.max_depth = depth;
            config.crawler.max_concurrent = concurrent;
            config.data_dir = output_dir.clone();
            config.index.data_dir = format!("{output_dir}/index");
            config.embedder.data_dir = format!("{output_dir}/vectors");
            config.storage.max_gb = max_storage_gb;
            config.storage.warning_threshold_gb = warning_threshold_gb;
            pipeline::crawl(config, urls).await
        }

        Commands::Search {
            query,
            limit,
            mode,
            output_dir,
        } => {
            config.data_dir = output_dir.clone();
            config.index.data_dir = format!("{output_dir}/index");
            config.embedder.data_dir = format!("{output_dir}/vectors");
            let sq = SearchQuery { text: query, limit };
            pipeline::search(config, sq, mode.into()).await
        }

        Commands::Stats { output_dir } => {
            config.data_dir = output_dir.clone();
            config.index.data_dir = format!("{output_dir}/index");
            config.embedder.data_dir = format!("{output_dir}/vectors");
            pipeline::stats(config).await
        }

        Commands::Serve {
            port,
            host,
            output_dir,
        } => {
            config.data_dir = output_dir.clone();
            config.index.data_dir = format!("{output_dir}/index");
            config.embedder.data_dir = format!("{output_dir}/vectors");
            api::serve(config, &host, port).await
        }

        Commands::Config { show, init } => pipeline::config_cmd(config, show, init).await,
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
        match std::fs::read_to_string(&candidate) {
            Ok(contents) => match serde_json::from_str::<SlitherConfig>(&contents) {
                Ok(cfg) => return cfg,
                Err(e) => {
                    eprintln!("Warning: failed to parse config at {candidate}: {e}");
                }
            },
            Err(_) => {}
        }
    }

    SlitherConfig::default()
}
