//! HTTP API handlers: health, metrics, restore.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};

use ci_cache_core::manifest::dto::*;

use crate::AppState;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn readyz(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    (StatusCode::OK, "ready")
}

pub async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let families = state.metrics.registry.gather();
    let mut buf = Vec::new();
    encoder.encode(&families, &mut buf).ok();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        String::from_utf8_lossy(&buf).to_string(),
    )
}

pub async fn restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RestoreRequest>,
) -> Response {
    state.metrics.restore_requests.inc();
    let timer = state.metrics.restore_duration.start_timer();

    if let Err(e) = ci_cache_core::paths::validate_namespace(&req.namespace) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    if let Err(e) = ci_cache_core::paths::validate_key(&req.key) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    match state.backend.get_manifest(&req.namespace, &req.key).await {
        Ok(Some(manifest)) => {
            if manifest.cache_type != req.cache_type {
                state.metrics.misses.inc();
                timer.stop_and_record();
                return (
                    StatusCode::OK,
                    Json(RestoreResponse { hit: false, manifest: None }),
                )
                    .into_response();
            }
            if manifest.is_expired(chrono::Utc::now()) {
                state.metrics.misses.inc();
                timer.stop_and_record();
                return (
                    StatusCode::OK,
                    Json(RestoreResponse { hit: false, manifest: None }),
                )
                    .into_response();
            }
            state.metrics.hits.inc();
            let bytes = manifest.total_compressed_size();
            state.metrics.bytes_downloaded.inc_by(bytes);
            timer.stop_and_record();
            (
                StatusCode::OK,
                Json(RestoreResponse { hit: true, manifest: Some(manifest) }),
            )
                .into_response()
        }
        Ok(None) => {
            state.metrics.misses.inc();
            timer.stop_and_record();
            (
                StatusCode::OK,
                Json(RestoreResponse { hit: false, manifest: None }),
            )
                .into_response()
        }
        Err(e) => {
            state.metrics.backend_errors.inc();
            timer.stop_and_record();
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
