//! HTTP client for communicating with ci-cache-server.

use ci_cache_core::manifest::dto::*;
use ci_cache_core::manifest::{BlobRef, CachedPath};

#[derive(Clone)]
pub struct CacheClient {
    base_url: String,
    http: reqwest::Client,
    token: Option<String>,
}

impl CacheClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            token,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref t) = self.token {
            req.bearer_auth(t)
        } else {
            req
        }
    }

    pub async fn restore(
        &self,
        namespace: &str,
        cache_type: ci_cache_core::CacheType,
        key: &str,
    ) -> anyhow::Result<RestoreResponse> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/cache/restore"))
                    .json(&RestoreRequest {
                        namespace: namespace.to_string(),
                        cache_type,
                        key: key.to_string(),
                    }),
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("restore failed: HTTP {}", resp.status());
        }
        Ok(resp.json().await?)
    }

    pub async fn save_start(
        &self,
        namespace: &str,
        cache_type: ci_cache_core::CacheType,
        key: &str,
        ttl_seconds: Option<u64>,
        metadata: std::collections::HashMap<String, String>,
    ) -> anyhow::Result<String> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/cache/save/start"))
                    .json(&SaveStartRequest {
                        namespace: namespace.to_string(),
                        cache_type,
                        key: key.to_string(),
                        ttl_seconds,
                        metadata,
                    }),
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("save/start failed: HTTP {}", resp.status());
        }
        let r: SaveStartResponse = resp.json().await?;
        Ok(r.session_id)
    }

    pub async fn upload_blob(
        &self,
        session_id: &str,
        digest: &str,
        compression: &str,
        data: bytes::Bytes,
    ) -> anyhow::Result<BlobUploadResponse> {
        let resp = self
            .add_auth(
                self.http
                    .put(self.url("/v1/cache/save/blob"))
                    .query(&[
                        ("session_id", session_id),
                        ("digest", digest),
                        ("compression", compression),
                    ])
                    .header("Content-Type", "application/octet-stream")
                    .body(data),
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("blob upload failed: {text}");
        }
        Ok(resp.json().await?)
    }

    pub async fn has_blobs(
        &self,
        digests: &[String],
    ) -> anyhow::Result<BatchHasBlobsResponse> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/cache/has-blobs"))
                    .json(&BatchHasBlobsRequest {
                        digests: digests.to_vec(),
                    }),
            )
            .send()
            .await?;
        Ok(resp.json().await?)
    }

    pub async fn save_finish(
        &self,
        session_id: &str,
        paths: Vec<CachedPath>,
        blobs: Vec<BlobRef>,
    ) -> anyhow::Result<SaveFinishResponse> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/cache/save/finish"))
                    .json(&SaveFinishRequest {
                        session_id: session_id.to_string(),
                        paths,
                        blobs,
                    }),
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("save/finish failed: {text}");
        }
        Ok(resp.json().await?)
    }

    pub async fn get_blob(&self, digest: &str) -> anyhow::Result<bytes::Bytes> {
        let url = self.url(&format!("/v1/cache/blob/{}", digest));
        let resp = self.add_auth(self.http.get(url)).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("get blob failed: HTTP {}", resp.status());
        }
        Ok(resp.bytes().await?)
    }

    pub async fn health(&self) -> anyhow::Result<bool> {
        let resp = self.http.get(self.url("/healthz")).send().await?;
        Ok(resp.status().is_success())
    }
}
