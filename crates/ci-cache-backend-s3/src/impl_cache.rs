//! CacheBackend trait implementation for S3Backend.

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use futures::StreamExt;

use ci_cache_core::backend::{BlobLocation, ByteStream, CacheBackend};
use ci_cache_core::error::{CacheError, Result};
use ci_cache_core::manifest::CacheManifest;

use crate::backend::S3Backend;

#[async_trait]
impl CacheBackend for S3Backend {
    fn name(&self) -> &str {
        "s3"
    }

    async fn put_blob(&self, digest: &str, mut bytes: ByteStream) -> Result<BlobLocation> {
        let mut buf = Vec::new();
        let mut total: u64 = 0;
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            buf.extend_from_slice(&chunk);
        }
        let key = self.blob_key(digest);
        let url = self.object_url(&key);
        let resp = self
            .sign_and_send(reqwest::Method::PUT, &url, Some(Bytes::from(buf)))
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 PUT failed ({status}): {text}")));
        }
        Ok(BlobLocation::new("s3", key, total))
    }

    async fn get_blob(&self, digest: &str) -> Result<ByteStream> {
        let key = self.blob_key(digest);
        let url = self.object_url(&key);
        let resp = self.sign_and_send(reqwest::Method::GET, &url, None).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(CacheError::BlobNotFound(digest.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 GET failed ({status}): {text}")));
        }
        let stream = resp
            .bytes_stream()
            .map(|r| r.map_err(|e| CacheError::Backend(e.to_string())));
        Ok(Box::pin(stream))
    }

    async fn has_blob(&self, digest: &str) -> Result<bool> {
        let key = self.blob_key(digest);
        let url = self.object_url(&key);
        let resp = self.sign_and_send(reqwest::Method::HEAD, &url, None).await?;
        match resp.status() {
            s if s.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            s => {
                let text = resp.text().await.unwrap_or_default();
                Err(CacheError::Backend(format!("S3 HEAD failed ({s}): {text}")))
            }
        }
    }

    async fn delete_blob(&self, digest: &str) -> Result<()> {
        let key = self.blob_key(digest);
        let url = self.object_url(&key);
        let resp = self.sign_and_send(reqwest::Method::DELETE, &url, None).await?;
        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 DELETE failed ({status}): {text}")));
        }
        Ok(())
    }

    async fn put_manifest(&self, manifest: CacheManifest) -> Result<()> {
        let key = self.manifest_key(&manifest.namespace, &manifest.key);
        let url = self.object_url(&key);
        let json = serde_json::to_vec(&manifest)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        let resp = self
            .sign_and_send(reqwest::Method::PUT, &url, Some(Bytes::from(json)))
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 PUT manifest ({status}): {text}")));
        }
        Ok(())
    }

    async fn get_manifest(&self, namespace: &str, key: &str) -> Result<Option<CacheManifest>> {
        let obj_key = self.manifest_key(namespace, key);
        let url = self.object_url(&obj_key);
        let resp = self.sign_and_send(reqwest::Method::GET, &url, None).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 GET manifest ({status}): {text}")));
        }
        let body = resp.bytes().await.map_err(|e| CacheError::Backend(e.to_string()))?;
        let manifest: CacheManifest = serde_json::from_slice(&body)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        Ok(Some(manifest))
    }

    async fn delete_manifest(&self, namespace: &str, key: &str) -> Result<()> {
        let obj_key = self.manifest_key(namespace, key);
        let url = self.object_url(&obj_key);
        let resp = self.sign_and_send(reqwest::Method::DELETE, &url, None).await?;
        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CacheError::Backend(format!("S3 DELETE manifest ({status}): {text}")));
        }
        Ok(())
    }
}
