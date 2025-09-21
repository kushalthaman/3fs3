pub mod config;
pub mod mount;
pub mod s3;
pub mod storage;

use axum::{routing::{get, put, post, delete, head}, Router};
use std::net::SocketAddr;
use tracing::{info};
use crate::config::GatewayConfig;
use crate::s3::handlers;
use crate::s3::auth::SigV4Layer;
use tower_http::{trace::TraceLayer, cors::CorsLayer};
use prometheus::{Encoder, TextEncoder, Registry, IntCounter, HistogramOpts, Histogram};
use once_cell::sync::Lazy;
use axum::response::IntoResponse;

#[derive(Clone)]
pub struct AppState {
    pub cfg: GatewayConfig,
    pub registry: Registry,
    pub req_counter: IntCounter,
    pub req_latency: Histogram,
}

static GLOBAL_REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::permissive();
    let healthz = get(|| async { "ok" });
    let readyz_state = state.cfg.clone();
    let readyz = get(move || {
        let cfg = readyz_state.clone();
        async move {
            if crate::mount::is_mounted_and_writeable(&cfg.mountpoint).await { "ready".into_response() } else { (axum::http::StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response() }
        }
    });

    let metrics_registry = state.registry.clone();
    let metrics = get(move || {
        let registry = metrics_registry.clone();
        async move {
            let mut buf = Vec::new();
            let encoder = TextEncoder::new();
            let mf = registry.gather();
            encoder.encode(&mf, &mut buf).unwrap();
            let ct = encoder.format_type().to_string();
            ([("Content-Type", ct)], buf)
        }
    });

    let s3_routes = Router::new()
        .route("/", get(handlers::bucket_list))
        .route("/", head(handlers::service_root))
        .route("/", post(handlers::service_root))
        .route("/", delete(handlers::service_root))
        .route("/", put(handlers::service_root))
        .route("/", axum::routing::options(handlers::cors_preflight))
        .route("/:bucket", put(handlers::create_bucket)
            .head(handlers::head_bucket)
            .delete(handlers::delete_bucket)
            .get(handlers::list_objects_v2))
        .route("/:bucket", post(handlers::bucket_post))
        .route("/:bucket/*key", put(handlers::put_object).get(handlers::get_object).head(handlers::head_object).delete(handlers::delete_object).post(handlers::object_post))
        .route("/:bucket/*key", axum::routing::options(handlers::cors_preflight))
        ;

    Router::new()
        .route("/healthz", healthz)
        .route("/readyz", readyz)
        .route("/metrics", metrics)
        .nest("/", s3_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(SigV4Layer::new(state.cfg.clone()))
        .with_state(state)
}

pub async fn run_server(cfg: GatewayConfig) -> anyhow::Result<()> {
    crate::mount::ensure_mount(&cfg).await?;
    crate::storage::posix::ensure_roots(&cfg).await?;

    let registry = GLOBAL_REGISTRY.clone();
    let req_counter = IntCounter::new("http_requests_total", "Total HTTP requests").unwrap();
    registry.register(Box::new(req_counter.clone())).ok();
    let req_latency = Histogram::with_opts(HistogramOpts::new("http_request_duration_seconds", "Request latencies")).unwrap();
    registry.register(Box::new(req_latency.clone())).ok();

    let state = AppState { cfg: cfg.clone(), registry, req_counter, req_latency };
    let app = build_router(state);

    let addr: SocketAddr = cfg.bind_addr.parse()?;
    info!(%addr, "binding");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

