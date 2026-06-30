//! Filesystem/PVC backend implementation.
//!
//! Stores content-addressed blobs and JSON manifests on a mounted volume.
//! Layout:
//!   {root}/blobs/{digest_hex}
//!   {root}/blobs/{digest_hex}.tmp.{uuid}  (atomic writes)
//!   {root}/manifests/{namespace}/{key}.json
//!   {root}/manifests/{namespace}/{key}.json.tmp.{uuid}

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use futures::StreamExt;
use tokio::io::AsyncReadExt;

use ci_cache_core::backend::{BlobLocation, ByteStream, CacheBackend};
use ci_cache_core::digest::digest_hex;
use ci_cache_core::error::{CacheError, Result};
use ci_cache_core::manifest::CacheManifest;

/// Filesystem/PVC backend storing blobs and manifests on a local volume.
pub struct FsBackend {
    root: PathBuf,
    blobs_dir: PathBuf,
    manifests_dir: PathBuf,
}

impl FsBackend {
    /// Create a new FS backend rooted at `root`. Creates the directory tree.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let blobs_dir = root.join("blobs");
        let manifests_dir = root.join("manifests");
        std::fs::create_dir_all(&blobs_dir)?;
        std::fs::create_dir_all(&manifests_dir)?;
        Ok(Self { root, blobs_dir, manifests_dir })
    }

    fn blob_path(&self, digest: &str) -> PathBuf {
        self.blobs_dir.join(digest_hex(digest))
    }

    fn manifest_path(&self, namespace: &str, key: &str) -> Result<PathBuf> {
        ci_cache_core::paths::validate_namespace(namespace)?;
        ci_cache_core::paths::validate_key(key)?;
        // Sanitize namespace for filesystem use (it may contain '/').
        let safe_ns = namespace.replace('/', "__");
        let dir = self.manifests_dir.join(&safe_ns);
        let safe_key = key.replace('/', "__");
        Ok(dir.join(format!("{safe_key}.json")))
    }
}

fn io_to_cache(e: std::io::Error) -> CacheError {
    if e.kind() == std::io::ErrorKind::NotFound {
        CacheError::BlobNotFound(e.to_string())
    } else {
        CacheError::Io(e)
    }
}

#[async_trait]
impl CacheBackend for FsBackend {
    fn name(&self) -> &str {
        "local"
    }

    async fn put_blob(
        &self,
        digest: &str,
        mut bytes: ByteStream,
    ) -> Result<BlobLocation> {
        let final_path = self.blob_path(digest);

        // If already exists, we can short-circuit (dedup).
        if final_path.exists() {
            let size = tokio::fs::metadata(&final_path).await?.len();
            return Ok(BlobLocation::new(
                "local",
                final_path.to_string_lossy().to_string(),
                size,
            ));
        }

        // Write to a temp file then rename atomically.
        let tmp = final_path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut total: u64 = 0;
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await?;
        drop(file);

        // Atomic rename.
        tokio::fs::rename(&tmp, &final_path).await.map_err(|e| {
            // Clean up temp on failure.
            let tmp_clone = tmp.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(&tmp_clone).await;
            });
            CacheError::Backend(format!("rename failed: {e}"))
        })?;

        Ok(BlobLocation::new(
            "local",
            final_path.to_string_lossy().to_string(),
            total,
        ))
    }

    async fn get_blob(&self, digest: &str) -> Result<ByteStream> {
        let path = self.blob_path(digest);
        if !path.exists() {
            return Err(CacheError::BlobNotFound(digest.to_string()));
        }
        let file = tokio::fs::File::open(&path).await.map_err(io_to_cache)?;
        let stream = tokio_util::io::ReaderStream::new(file)
            .map(|r| r.map(Bytes::from).map_err(CacheError::from));
        Ok(Box::pin(stream))
    }

    async fn get_blob_bytes(&self, digest: &str) -> Result<Bytes> {
        let path = self.blob_path(digest);
        let mut file = tokio::fs::File::open(&path).await.map_err(io_to_cache)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(io_to_cache)?;
        Ok(Bytes::from(buf))
    }

    async fn has_blob(&self, digest: &str) -> Result<bool> {
        Ok(self.blob_path(digest).exists())
    }

    async fn delete_blob(&self, digest: &str) -> Result<()> {
        let path = self.blob_path(digest);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    async fn put_manifest(&self, manifest: CacheManifest) -> Result<()> {
        let path = self.manifest_path(&manifest.namespace, &manifest.key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        let tmp = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp, &json).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    async fn get_manifest(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<CacheManifest>> {
        let path = self.manifest_path(namespace, key)?;
        match tokio::fs::read(&path).await {
            Ok(data) => {
                let manifest: CacheManifest = serde_json::from_slice(&data)
                    .map_err(|e| CacheError::Serialization(e.to_string()))?;
                Ok(Some(manifest))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    async fn delete_manifest(&self, namespace: &str, key: &str) -> Result<()> {
        let path = self.manifest_path(namespace, key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CacheError::Io(e)),
        }
    }
}
