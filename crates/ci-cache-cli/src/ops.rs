//! CLI restore/save operations.

use ci_cache_core::archive_ops::extract_archive;
use ci_cache_core::archive::create_archive_from_dir;
use ci_cache_core::compression::Compression;
use ci_cache_core::digest::{compute_digest, digest_hex};
use ci_cache_core::manifest::{BlobRef, CachedPath};
use ci_cache_core::paths;

use crate::client::CacheClient;

/// Result of a restore operation.
pub struct RestoreResult {
    pub hit: bool,
    pub bytes_downloaded: u64,
    pub files_restored: usize,
}

/// Restore a cache entry from the server.
pub async fn restore(
    client: &CacheClient,
    namespace: &str,
    cache_type: ci_cache_core::CacheType,
    key: &str,
    path_specs: &[String],
    max_decompress: u64,
) -> anyhow::Result<RestoreResult> {
    let resp = client.restore(namespace, cache_type, key).await?;

    if !resp.hit {
        tracing::info!("cache MISS for key={}", key);
        return Ok(RestoreResult {
            hit: false,
            bytes_downloaded: 0,
            files_restored: 0,
        });
    }

    let manifest = resp
        .manifest
        .ok_or_else(|| anyhow::anyhow!("server reported hit but sent no manifest"))?;

    tracing::info!(
        key = %key,
        blobs = manifest.blobs.len(),
        "cache HIT, restoring"
    );

    let mut total_bytes: u64 = 0;
    let mut total_files: usize = 0;

    for cached_path in &manifest.paths {
        let dest = paths::resolve_path(&cached_path.original_path)?;
        let blob_ref = manifest
            .find_blob(&cached_path.archive_digest)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "manifest references missing blob: {}",
                    cached_path.archive_digest
                )
            })?;

        let compression = Compression::parse(&blob_ref.compression)
            .map_err(|e| anyhow::anyhow!("bad compression: {e}"))?;

        tracing::debug!(
            path = ?dest,
            digest = %cached_path.archive_digest,
            "downloading blob"
        );

        let blob_bytes = client.get_blob(&cached_path.archive_digest).await?;

        // Verify digest before extraction.
        let actual = compute_digest(&blob_bytes);
        if actual != cached_path.archive_digest {
            anyhow::bail!(
                "digest mismatch on restore: expected={}, actual={}",
                cached_path.archive_digest,
                actual
            );
        }

        let header = extract_archive(&blob_bytes, compression, &dest, max_decompress)?;
        total_bytes += blob_bytes.len() as u64;
        total_files += header.entries.len();

        #[cfg(unix)]
        if let Some(mode) = cached_path.mode {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode));
        }
    }

    tracing::info!(bytes = total_bytes, files = total_files, "restore complete");

    Ok(RestoreResult {
        hit: true,
        bytes_downloaded: total_bytes,
        files_restored: total_files,
    })
}

/// Result of a save operation.
pub struct SaveResult {
    pub key: String,
    pub bytes_uploaded: u64,
    pub bytes_deduped: u64,
    pub blob_count: usize,
}

/// Save a cache entry to the server.
pub async fn save(
    client: &CacheClient,
    namespace: &str,
    cache_type: ci_cache_core::CacheType,
    key: &str,
    path_specs: &[String],
    compression: Compression,
    max_archive: u64,
    ttl_seconds: Option<u64>,
) -> anyhow::Result<SaveResult> {
    let session_id = client
        .save_start(namespace, cache_type, key, ttl_seconds, Default::default())
        .await?;

    tracing::info!(session_id = %session_id, "save session started");

    let mut all_blobs: Vec<BlobRef> = Vec::new();
    let mut cached_paths: Vec<CachedPath> = Vec::new();

    for spec in path_specs {
        let resolved = paths::resolve_path(spec)?;

        if !resolved.exists() {
            tracing::warn!(path = ?resolved, "path does not exist, skipping");
            continue;
        }

        if !resolved.is_dir() {
            tracing::warn!(path = ?resolved, "not a directory, skipping");
            continue;
        }

        tracing::info!(path = ?resolved, "archiving");

        let (compressed, header) =
            create_archive_from_dir(&resolved, compression, max_archive)?;

        let uncompressed_size = header.total_size;
        let digest = compute_digest(&compressed);

        let upload_resp = client
            .upload_blob(
                &session_id,
                &digest,
                compression.as_str(),
                bytes::Bytes::from(compressed),
            )
            .await?;

        let blob = BlobRef {
            digest: digest.clone(),
            size_bytes: upload_resp.size_bytes,
            uncompressed_size_bytes: uncompressed_size,
            compression: compression.as_str().to_string(),
            backend: "server".to_string(),
            location: format!("blobs/{}", digest_hex(&digest)),
        };

        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(&resolved).ok().map(|m| m.permissions().mode())
        };
        #[cfg(not(unix))]
        let mode = None;

        cached_paths.push(CachedPath {
            original_path: spec.clone(),
            archive_digest: digest.clone(),
            mode,
        });

        all_blobs.push(blob);
    }

    let result = client
        .save_finish(&session_id, cached_paths, all_blobs)
        .await?;

    Ok(SaveResult {
        key: result.key,
        bytes_uploaded: result.bytes_uploaded,
        bytes_deduped: result.bytes_deduped,
        blob_count: result.blob_count,
    })
}
