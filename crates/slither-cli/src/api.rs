use axum::{
    extract::Query,
    extract::Request,
    extract::State,
    http::{header, HeaderName, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use iris::Iris;
use serde::{Deserialize, Serialize};
use slither_core::{
    CrawlScope, SearchMode, SearchQuery, SearchResult, Seed, SeedConfig, SeedPriority,
    SlitherConfig, SlitherError, SlitherResult,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tome::Index;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use venom::Ranker;

use crate::seeds;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
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

#[derive(Clone)]
struct FaviconState {
    favicon_dir: PathBuf,
}

#[derive(Clone)]
struct AdminState {
    config: SlitherConfig,
    ranker: SharedRanker,
    is_crawling: Arc<AtomicBool>,
    last_crawl: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[derive(Debug, Serialize)]
struct AdminStatus {
    crawling: bool,
    last_crawl: Option<String>,
    seed_count: usize,
    doc_count: usize,
}

#[derive(Debug, Deserialize)]
struct SeedRequest {
    url: String,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    priority: Option<SeedPriority>,
    #[serde(default)]
    scope: Option<CrawlScope>,
    #[serde(default)]
    sitemap: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct CrawlRequest {
    urls: Option<Vec<String>>,
}

async fn auth_middleware(State(state): State<AdminState>, req: Request, next: Next) -> Response {
    let api_key = req.headers().get("X-Api-Key").and_then(|v| v.to_str().ok());

    match (&state.config.admin_key, api_key) {
        (Some(expected), Some(provided)) if expected == provided => next.run(req).await,
        _ => (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::err("unauthorized")),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub offset: usize,
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

type SharedRanker = Arc<tokio::sync::RwLock<Ranker>>;

pub async fn serve(config: SlitherConfig, host: &str, port: u16) -> SlitherResult<()> {
    println!("Starting API server on http://{}:{}", host, port);

    let index = Index::open_for_serve(std::path::Path::new(&config.index.data_dir))?;
    let embedder = Iris::new(config.embedder.clone())?;
    let ranker = Ranker::new(index, embedder);

    let shared_rankers = Arc::new(tokio::sync::RwLock::new(ranker));

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            "https://search.peril.lol".parse().unwrap(),
            "http://localhost:3000".parse().unwrap(),
            "http://localhost:5000".parse().unwrap(),
            "http://localhost:8000".parse().unwrap(),
            "http://localhost:8080".parse().unwrap(),
            "http://127.0.0.1:3000".parse().unwrap(),
            "http://127.0.0.1:5000".parse().unwrap(),
            "http://127.0.0.1:8000".parse().unwrap(),
            "http://127.0.0.1:8080".parse().unwrap(),
        ]))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, HeaderName::from_static("x-api-key")]);

    let static_dir = format!("{}/static", config.data_dir);
    std::fs::create_dir_all(&static_dir).ok();
    let index_file = format!("{}/index.html", static_dir);

    let favicon_dir = PathBuf::from(&config.data_dir).join("favicons");
    std::fs::create_dir_all(&favicon_dir).ok();

    let search_router = Router::new()
        .route("/search", get(search_handler))
        .route("/suggest", get(suggest_handler))
        .route("/stats", get(stats_handler))
        .route("/health", get(health_handler))
        .with_state(shared_rankers.clone());

    let favicon_router = Router::new()
        .route("/favicon", get(favicon_handler))
        .with_state(FaviconState { favicon_dir });

    let admin_state = AdminState {
        config: config.clone(),
        ranker: shared_rankers,
        is_crawling: Arc::new(AtomicBool::new(false)),
        last_crawl: Arc::new(tokio::sync::Mutex::new(None)),
    };

    let admin_router = Router::new()
        .route(
            "/admin/seeds",
            get(admin_seeds_handler)
                .post(admin_add_seed_handler)
                .delete(admin_remove_seed_handler),
        )
        .route("/admin/crawl", post(admin_crawl_handler))
        .route("/admin/status", get(admin_status_handler))
        .route("/admin/reload", post(admin_reload_handler))
        .layer(axum::middleware::from_fn_with_state(
            admin_state.clone(),
            auth_middleware,
        ))
        .with_state(admin_state);

    let app = Router::new()
        .merge(search_router)
        .merge(favicon_router)
        .merge(admin_router)
        .layer(cors);

    let app = if std::path::Path::new(&index_file).exists() {
        let serve_dir = ServeDir::new(&static_dir).not_found_service(ServeFile::new(&index_file));
        app.fallback_service(serve_dir)
    } else {
        app
    };

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| SlitherError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("API server running at http://{}:{}", host, port);
    println!("Endpoints:");
    println!("  GET /search?q=<query>&limit=<n>&mode=<text|semantic|hybrid>&offset=<n>");
    println!("  GET /suggest?q=<query>&limit=<n>");
    println!("  GET /stats");
    println!("  GET /health");
    println!("  GET /favicon?domain=<domain>");
    println!("  GET /admin/seeds (X-Api-Key required)");
    println!("  POST /admin/seeds (X-Api-Key required)");
    println!("  DELETE /admin/seeds (X-Api-Key required)");
    println!("  POST /admin/crawl (X-Api-Key required)");
    println!("  GET /admin/status (X-Api-Key required)");
    println!("  POST /admin/reload (X-Api-Key required) - reload index without restart");
    if std::path::Path::new(&index_file).exists() {
        println!("  Static files: {}", static_dir);
    } else {
        println!("  (no static dir at {} — API only)", static_dir);
    }

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

    let fetch_limit = limit + params.offset;
    let query = SearchQuery {
        text: params.q.clone(),
        limit: fetch_limit,
    };

    let mut ranker = rankers.write().await;

    match ranker.search(&query, mode) {
        Ok(results) => {
            let json_results: Vec<SearchResultJson> = results
                .into_iter()
                .skip(params.offset)
                .take(limit)
                .map(|r| r.into())
                .collect();
            Json(ApiResponse::ok(json_results))
        }
        Err(e) => Json(ApiResponse::err(&e.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct SuggestParams {
    q: String,
    #[serde(default = "default_suggest_limit")]
    limit: usize,
}

fn default_suggest_limit() -> usize {
    5
}

async fn suggest_handler(
    Query(params): Query<SuggestParams>,
    State(rankers): State<SharedRanker>,
) -> Json<ApiResponse<Vec<SearchResultJson>>> {
    let q = params.q.trim();
    if q.len() < 2 {
        return Json(ApiResponse::ok(Vec::new()));
    }
    let limit = if params.limit == 0 { 5 } else { params.limit.min(10) };
    let ranker = rankers.read().await;
    let results = ranker.index.search(q, limit);
    match results {
        Ok(results) => {
            let json_results: Vec<SearchResultJson> = results.into_iter().map(|r| r.into()).collect();
            Json(ApiResponse::ok(json_results))
        }
        Err(_) => Json(ApiResponse::ok(Vec::new())),
    }
}

async fn stats_handler(
    State(rankers): State<SharedRanker>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ranker = rankers.read().await;
    let doc_count = ranker.doc_count();

    Json(ApiResponse::ok(serde_json::json!({
        "documents": doc_count,
    })))
}

async fn health_handler() -> Json<ApiResponse<String>> {
    Json(ApiResponse::ok("ok".to_string()))
}

async fn admin_seeds_handler(State(state): State<AdminState>) -> Json<ApiResponse<Vec<Seed>>> {
    let seeds = seeds::load_seeds(&state.config.data_dir);
    Json(ApiResponse::ok(seeds))
}

async fn admin_add_seed_handler(
    State(state): State<AdminState>,
    Json(body): Json<SeedRequest>,
) -> Json<ApiResponse<Vec<Seed>>> {
    let mut seed_list = seeds::load_seeds(&state.config.data_dir);
    let new_seed = if body.depth.is_none()
        && body.priority.is_none()
        && body.scope.is_none()
        && body.sitemap.is_none()
    {
        Seed::Simple(body.url)
    } else {
        Seed::Configured(SeedConfig {
            url: body.url,
            depth: body.depth.unwrap_or(slither_core::default_seed_depth()),
            priority: body.priority.unwrap_or_default(),
            scope: body.scope.unwrap_or_default(),
            sitemap: body.sitemap.unwrap_or(false),
        })
    };
    if !seed_list.iter().any(|s| s.url() == new_seed.url()) {
        seed_list.push(new_seed);
    }
    match seeds::save_seeds(&state.config.data_dir, &seed_list) {
        Ok(()) => Json(ApiResponse::ok(seed_list)),
        Err(e) => Json(ApiResponse::err(&e.to_string())),
    }
}

async fn admin_remove_seed_handler(
    State(state): State<AdminState>,
    Json(body): Json<SeedRequest>,
) -> Json<ApiResponse<Vec<Seed>>> {
    let mut seed_list = seeds::load_seeds(&state.config.data_dir);
    seed_list.retain(|s| s.url() != body.url);
    match seeds::save_seeds(&state.config.data_dir, &seed_list) {
        Ok(()) => Json(ApiResponse::ok(seed_list)),
        Err(e) => Json(ApiResponse::err(&e.to_string())),
    }
}

async fn admin_crawl_handler(
    State(state): State<AdminState>,
    body: Option<Json<CrawlRequest>>,
) -> Json<ApiResponse<String>> {
    if state.is_crawling.load(Ordering::SeqCst) {
        return Json(ApiResponse::err("crawl already in progress"));
    }

    let seed_list: Vec<Seed> = match body {
        Some(Json(req)) if req.urls.as_ref().map(|u| !u.is_empty()).unwrap_or(false) => {
            req.urls.unwrap().into_iter().map(Seed::Simple).collect()
        }
        _ => seeds::load_seeds(&state.config.data_dir),
    };

    if seed_list.is_empty() {
        return Json(ApiResponse::err("no seed URLs configured"));
    }

    let is_crawling = state.is_crawling.clone();
    let last_crawl = state.last_crawl.clone();
    let config = state.config.clone();
    let ranker = state.ranker.clone();

    is_crawling.store(true, Ordering::SeqCst);

    tokio::spawn(async move {
        let _ = crate::pipeline::crawl(config.clone(), seed_list).await;

        // Auto-reload index so new data is served immediately
        if let Ok(new_index) = Index::open(std::path::Path::new(&config.index.data_dir)) {
            if let Ok(new_embedder) = Iris::new(config.embedder.clone()) {
                let new_ranker = Ranker::new(new_index, new_embedder);
                let mut guard = ranker.write().await;
                *guard = new_ranker;
                drop(guard);
                tracing::info!("index auto-reloaded after crawl");
            }
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let mut guard = last_crawl.lock().await;
        *guard = Some(ts);
        drop(guard);
        is_crawling.store(false, Ordering::SeqCst);
    });

    Json(ApiResponse::ok("crawl started".to_string()))
}

async fn admin_status_handler(State(state): State<AdminState>) -> Json<ApiResponse<AdminStatus>> {
    let crawling = state.is_crawling.load(Ordering::SeqCst);
    let last_crawl = state.last_crawl.lock().await.clone();
    let seed_count = seeds::load_seeds(&state.config.data_dir).len();
    let doc_count = state.ranker.read().await.doc_count();

    Json(ApiResponse::ok(AdminStatus {
        crawling,
        last_crawl,
        seed_count,
        doc_count,
    }))
}

async fn admin_reload_handler(State(state): State<AdminState>) -> Json<ApiResponse<String>> {
    let config = state.config.clone();
    let new_index = match Index::open(std::path::Path::new(&config.index.data_dir)) {
        Ok(idx) => idx,
        Err(e) => return Json(ApiResponse::err(&format!("failed to open index: {}", e))),
    };
    let new_embedder = match Iris::new(config.embedder.clone()) {
        Ok(emb) => emb,
        Err(e) => return Json(ApiResponse::err(&format!("failed to open embedder: {}", e))),
    };
    let new_ranker = Ranker::new(new_index, new_embedder);

    let mut guard = state.ranker.write().await;
    *guard = new_ranker;
    drop(guard);

    let doc_count = state.ranker.read().await.doc_count();
    Json(ApiResponse::ok(format!("index reloaded, {} documents", doc_count)))
}

#[derive(Debug, Deserialize)]
struct FaviconParams {
    domain: String,
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

fn detect_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF") {
        "image/gif"
    } else if bytes.starts_with(b"\xFF\xD8") {
        "image/jpeg"
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "image/svg+xml"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/x-icon"
    }
}

async fn favicon_handler(
    Query(params): Query<FaviconParams>,
    State(state): State<FaviconState>,
) -> Response {
    let clean = sanitize_domain(&params.domain);
    let path = state.favicon_dir.join(&clean);

    match std::fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => {
            let ct = detect_content_type(&bytes);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, ct),
                    (header::CACHE_CONTROL, "public, max-age=604800, immutable"),
                ],
                bytes,
            )
                .into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
