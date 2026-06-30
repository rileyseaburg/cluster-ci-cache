//! ci-cache-server: in-cluster cache server.

mod api;
mod api_rest;
mod api_save;
mod backend_factory;
mod session;

use std::sync::Arc;

use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::trace::TraceLayer;

use ci_cache_core::backend::CacheBackend;
use ci_cache_core::config_extra::AppConfig;
use ci_cache_core::metrics::Metrics;

use crate::session::SessionManager;

/// Shared application state.
pub struct AppState {
    pub backend: Box<dyn CacheBackend>,
    pub config: AppConfig,
    pub metrics: Metrics,
    pub sessions: SessionManager,
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(api::healthz))
        .route("/readyz", get(api::readyz))
        .route("/metrics", get(api::metrics))
        .route("/v1/cache/restore", post(api::restore))
        .route("/v1/cache/save/start", post(api_save::save_start))
        .route("/v1/cache/save/blob", put(api_save::save_blob))
        .route("/v1/cache/save/finish", post(api_rest::save_finish))
        .route("/v1/cache/manifest/:namespace/:key", get(api_rest::get_manifest))
        .route("/v1/cache/manifest/:namespace/:key", delete(api_rest::delete_manifest))
        .route("/v1/cache/blob/:digest", get(api_rest::get_blob))
        .route("/v1/cache/blob/:digest", delete(api_rest::delete_blob))
        .route("/v1/cache/blob/:digest/exists", get(api_rest::blob_exists))
        .route("/v1/cache/has-blobs", post(api_rest::has_blobs))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = AppConfig::from_env_or_default();
    tracing::info!(addr = %config.server.listen_addr, "starting ci-cache-server");

    let backend = backend_factory::create_backend(&config)?;
    let state = Arc::new(AppState {
        backend,
        config: config.clone(),
        metrics: Metrics::new(),
        sessions: SessionManager::new(),
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&config.server.listen_addr).await?;
    tracing::info!("listening on {}", config.server.listen_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
