use axum::{
    extract::Query,
    extract::State,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use slither_core::{SearchMode, SearchQuery, SearchResult, SlitherConfig, SlitherError, SlitherResult};
use venom::Ranker;
use tome::Index;
use iris::Iris;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: &str) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.to_string()),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub mode: String,
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct SearchResultJson {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub score: f32,
}

impl From<SearchResult> for SearchResultJson {
    fn from(r: SearchResult) -> Self {
        Self {
            title: r.title,
            url: r.url,
            snippet: r.snippet,
            score: r.score,
        }
    }
}

type SharedRanker = Arc<tokio::sync::Mutex<Ranker>>;

pub async fn serve(config: SlitherConfig, host: &str, port: u16) -> SlitherResult<()> {
    println!("Starting API server on http://{}:{}", host, port);

    let index = Index::open(std::path::Path::new(&config.index.data_dir))?;
    let embedder = Iris::new(config.embedder.clone())?;
    let ranker = Ranker::new(index, embedder);

    let shared_rankers = Arc::new(tokio::sync::Mutex::new(ranker));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/search", get(search_handler))
        .route("/stats", get(stats_handler))
        .route("/health", get(health_handler))
        .layer(cors)
        .with_state(shared_rankers);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()
        .map_err(|e| SlitherError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("API server running at http://{}:{}", host, port);
    println!("Endpoints:");
    println!("  GET /search?q=<query>&limit=<n>&mode=<text|semantic|hybrid>");
    println!("  GET /stats");
    println!("  GET /health");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn search_handler(
    Query(params): Query<SearchParams>,
    State(rankers): State<SharedRanker>,
) -> Json<ApiResponse<Vec<SearchResultJson>>> {
    let limit = if params.limit == 0 { 10 } else { params.limit };

    let mode = match params.mode.as_str() {
        "text" => SearchMode::Text,
        "semantic" => SearchMode::Semantic,
        "hybrid" => SearchMode::Hybrid,
        _ => SearchMode::Hybrid,
    };

    let query = SearchQuery {
        text: params.q.clone(),
        limit,
    };

    let ranker = rankers.lock().await;

    match ranker.search(&query, mode) {
        Ok(results) => {
            let json_results: Vec<SearchResultJson> = results.into_iter().map(|r| r.into()).collect();
            Json(ApiResponse::ok(json_results))
        }
        Err(e) => Json(ApiResponse::err(&e.to_string())),
    }
}

async fn stats_handler(
    State(rankers): State<SharedRanker>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ranker = rankers.lock().await;
    let doc_count = ranker.index.doc_count();

    Json(ApiResponse::ok(serde_json::json!({
        "documents": doc_count,
    })))
}

async fn health_handler() -> Json<ApiResponse<String>> {
    Json(ApiResponse::ok("ok".to_string()))
}
