//! Backend abstraction for blob and manifest storage.

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;

use crate::error::Result;
use crate::manifest::CacheManifest;

/// A streaming byte source/sink used for blob transfer.
pub type ByteStream = BoxStream<'static, Result<Bytes>>;

/// Where a blob ended up being stored.
#[derive(Debug, Clone)]
pub struct BlobLocation {
    /// Backend name (e.g. "local", "s3").
    pub backend: String,
    /// Backend-specific location string (e.g. object key or file path).
    pub location: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

impl BlobLocation {
    pub fn new(backend: impl Into<String>, location: impl Into<String>, size_bytes: u64) -> Self {
        Self {
            backend: backend.into(),
            location: location.into(),
            size_bytes,
        }
    }
}

/// The core storage backend trait.
///
/// Backends are responsible for:
/// - content-addressed blob storage (put/get/has/delete)
/// - manifest storage keyed by (namespace, key)
///
/// Implementations must be safe for concurrent access.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Human-readable backend name (e.g. "local", "s3").
    fn name(&self) -> &str;

    // ---- Blobs (content-addressed) ----

    /// Store a blob. The caller must have already computed and verified the
    /// digest. Implementations should store under the digest so identical
    /// content deduplicates.
    async fn put_blob(
        &self,
        digest: &str,
        bytes: ByteStream,
    ) -> Result<BlobLocation>;

    /// Retrieve a blob as a stream of bytes.
    async fn get_blob(&self, digest: &str) -> Result<ByteStream>;

    /// Retrieve a blob fully into memory. Default implementation drains the
    /// stream. Backends with an efficient in-memory path may override.
    async fn get_blob_bytes(&self, digest: &str) -> Result<Bytes> {
        let stream = self.get_blob(digest).await?;
        collect_stream(stream).await
    }

    /// Check if a blob exists.
    async fn has_blob(&self, digest: &str) -> Result<bool>;

    /// Delete a blob.
    async fn delete_blob(&self, digest: &str) -> Result<()>;

    // ---- Manifests ----

    /// Atomically publish a manifest.
    async fn put_manifest(&self, manifest: CacheManifest) -> Result<()>;

    /// Fetch a manifest by namespace + key.
    async fn get_manifest(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<CacheManifest>>;

    /// Delete a manifest by namespace + key.
    async fn delete_manifest(&self, namespace: &str, key: &str) -> Result<()>;
}

/// Drain a [`ByteStream`] into a single [`Bytes`] buffer with a size cap to
/// guard against memory exhaustion.
pub async fn collect_stream_capped(stream: ByteStream, max_bytes: u64) -> Result<Bytes> {
    use futures::StreamExt;
    let mut stream = stream;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if (buf.len() + chunk.len()) as u64 > max_bytes {
            return Err(crate::error::CacheError::BlobTooLarge {
                size: (buf.len() + chunk.len()) as u64,
                limit: max_bytes,
            });
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(buf))
}

/// Drain a [`ByteStream`] into a single [`Bytes`] buffer with a generous
/// default cap.
pub async fn collect_stream(stream: ByteStream) -> Result<Bytes> {
    // 2 GiB default safety cap for in-memory collects.
    collect_stream_capped(stream, 2 * 1024 * 1024 * 1024).await
}
