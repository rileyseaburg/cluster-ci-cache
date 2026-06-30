//! Additional API handlers: save session, blobs, manifest CRUD.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post, put};
use futures::stream;
use serde::Deserialize;

use ci_cache_core::manifest::dto::*;
use ci_cache_core::manifest::{BlobRef, CacheManifest};

use crate::session::SaveSession;
use crate::AppState;

pub async fn save_start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveStartRequest>,
) -> Response {
    state.metrics.save_requests.inc();

    if let Err(e) = ci_cache_core::paths::validate_namespace(&req.namespace) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    if let Err(e) = ci_cache_core::paths::validate_key(&req.key) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let session = SaveSession::from_start(req, session_id.clone());
    state.sessions.create(session).await;

    (StatusCode::OK, Json(SaveStartResponse { session_id })).into_response()
}

#[derive(Deserialize)]
pub struct BlobUploadQuery {
    pub session_id: String,
    pub digest: String,
    pub compression: String,
}

pub async fn save_blob(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BlobUploadQuery>,
    body: Bytes,
) -> Response {
    let session = match state.sessions.get(&q.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                "session not found".to_string(),
            )
                .into_response()
        }
    };

    let max_blob = state.config.cache.max_blob_bytes();
    if body.len() as u64 > max_blob {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("blob exceeds max size {}", max_blob),
        )
            .into_response();
    }

    // Verify digest of the uploaded content.
    let actual_digest = ci_cache_core::digest::compute_digest(&body);
    if actual_digest != q.digest {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "digest mismatch: expected={}, actual={}",
                q.digest, actual_digest
            ),
        )
            .into_response();
    }

    // Check if blob already exists (dedup).
    let already_present = match state.backend.has_blob(&q.digest).await {
        Ok(p) => p,
        Err(e) => {
            state.metrics.backend_errors.inc();
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    let size_bytes = body.len() as u64;

    if already_present {
        state.metrics.blob_dedup_hits.inc();
    } else {
        // Stream body bytes into the backend.
        let bytes_vec = body.to_vec();
        let stream = stream::once(async move {
            Ok::<_, ci_cache_core::CacheError>(bytes::Bytes::from(bytes_vec))
        });
        let byte_stream: ci_cache_core::backend::ByteStream = Box::pin(stream);
        match state.backend.put_blob(&q.digest, byte_stream).await {
            Ok(loc) => {
                state.metrics.bytes_uploaded.inc_by(size_bytes);
            }
            Err(e) => {
                state.metrics.backend_errors.inc();
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }

    let blob = BlobRef {
        digest: q.digest.clone(),
        size_bytes,
        uncompressed_size_bytes: size_bytes,
        compression: q.compression.clone(),
        backend: state.backend.name().to_string(),
        location: format!("blobs/{}", ci_cache_core::digest::digest_hex(&q.digest)),
    };

    state
        .sessions
        .record_blob(&q.session_id, blob, already_present)
        .await;

    (
        StatusCode::OK,
        Json(BlobUploadResponse {
            stored: !already_present,
            already_present,
            digest: q.digest,
            size_bytes,
        }),
    )
        .into_response()
}
