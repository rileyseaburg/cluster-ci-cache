//! Remaining API handlers: save finish, blob retrieval, manifest/deletes.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};

use ci_cache_core::manifest::dto::*;
use ci_cache_core::manifest::CacheManifest;

use crate::AppState;

pub async fn save_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveFinishRequest>,
) -> Response {
    let session = match state.sessions.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (StatusCode::NOT_FOUND, "session not found".to_string()).into_response();
        }
    };

    let timer = state.metrics.save_duration.start_timer();

    // Use client-provided paths and blobs if provided; otherwise use session's.
    let blobs = if req.blobs.is_empty() {
        session.blobs.clone()
    } else {
        req.blobs
    };
    let paths = req.paths;

    let mut manifest = CacheManifest::new(
        session.namespace.clone(),
        session.cache_type,
        session.key.clone(),
    );
    manifest.ttl_seconds = Some(
        session
            .ttl_seconds
            .unwrap_or(state.config.cache.default_ttl_seconds),
    );
    manifest.metadata = session.metadata.clone();
    manifest.paths = paths;
    manifest.blobs = blobs;

    match state.backend.put_manifest(manifest.clone()).await {
        Ok(()) => {
            state.sessions.remove(&req.session_id).await;
            timer.stop_and_record();
            (
                StatusCode::OK,
                Json(SaveFinishResponse {
                    key: manifest.key,
                    bytes_uploaded: session.bytes_uploaded,
                    bytes_deduped: session.bytes_deduped,
                    blob_count: manifest.blobs.len(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            state.metrics.backend_errors.inc();
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_blob(
    State(state): State<Arc<AppState>>,
    Path(digest): Path<String>,
) -> Response {
    match state.backend.get_blob_bytes(&digest).await {
        Ok(bytes) => {
            state.metrics.bytes_downloaded.inc_by(bytes.len() as u64);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) if e.is_not_found() => (StatusCode::NOT_FOUND, "blob not found").into_response(),
        Err(e) => {
            state.metrics.backend_errors.inc();
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn delete_blob(
    State(state): State<Arc<AppState>>,
    Path(digest): Path<String>,
) -> Response {
    match state.backend.delete_blob(&digest).await {
        Ok(()) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn blob_exists(
    State(state): State<Arc<AppState>>,
    Path(digest): Path<String>,
) -> Response {
    match state.backend.has_blob(&digest).await {
        Ok(true) => (
            StatusCode::OK,
            Json(HasBlobResponse { present: true }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::OK,
            Json(HasBlobResponse { present: false }),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn has_blobs(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchHasBlobsRequest>,
) -> Response {
    let mut present = Vec::new();
    let mut absent = Vec::new();
    for digest in &req.digests {
        match state.backend.has_blob(digest).await {
            Ok(true) => present.push(digest.clone()),
            Ok(false) => absent.push(digest.clone()),
            Err(_) => absent.push(digest.clone()),
        }
    }
    (
        StatusCode::OK,
        Json(BatchHasBlobsResponse { present, absent }),
    )
        .into_response()
}

pub async fn get_manifest(
    State(state): State<Arc<AppState>>,
    Path((namespace, key)): Path<(String, String)>,
) -> Response {
    match state.backend.get_manifest(&namespace, &key).await {
        Ok(Some(m)) => (StatusCode::OK, Json(m)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "manifest not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_manifest(
    State(state): State<Arc<AppState>>,
    Path((namespace, key)): Path<(String, String)>,
) -> Response {
    match state.backend.delete_manifest(&namespace, &key).await {
        Ok(()) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
