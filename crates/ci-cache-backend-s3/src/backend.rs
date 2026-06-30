//! S3-compatible backend using reqwest with SigV4 signing.

use bytes::Bytes;

use ci_cache_core::error::{CacheError, Result};

use crate::signing;

/// S3-compatible backend (works with AWS S3, MinIO, etc.).
pub struct S3Backend {
    client: reqwest::Client,
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
    path_style: bool,
}

impl S3Backend {
    pub fn new(
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        path_style: bool,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| CacheError::Backend(e.to_string()))?;
        Ok(Self {
            client,
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            region: region.into(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            path_style,
        })
    }

    pub(crate) fn object_url(&self, key: &str) -> String {
        if self.path_style {
            format!("{}/{}/{}", self.endpoint.trim_end_matches('/'), self.bucket, key)
        } else {
            let host_base = self.endpoint
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/');
            format!("{}.{}/{}", self.bucket, host_base, key)
        }
    }

    pub(crate) fn blob_key(&self, digest: &str) -> String {
        format!("blobs/{}", ci_cache_core::digest::digest_hex(digest))
    }

    pub(crate) fn manifest_key(&self, namespace: &str, key: &str) -> String {
        let safe_ns = namespace.replace('/', "__");
        let safe_key = key.replace('/', "__");
        format!("manifests/{safe_ns}/{safe_key}.json")
    }

    pub(crate) async fn sign_and_send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<Bytes>,
    ) -> Result<reqwest::Response> {
        let parsed = url::Url::parse(url)
            .map_err(|e| CacheError::Backend(format!("bad url: {e}")))?;

        let body_hash = match &body {
            Some(b) => signing::hex_sha256(b),
            None => signing::hex_sha256(b"".as_ref()),
        };

        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let host = parsed.host_str().unwrap_or("");
        let port_suffix = match parsed.port() {
            Some(p) if (parsed.scheme() == "https" && p != 443)
                || (parsed.scheme() == "http" && p != 80) => format!(":{p}"),
            _ => String::new(),
        };
        let host_header = format!("{host}{port_suffix}");

        let canonical_uri = if self.path_style {
            format!("/{}/{}", self.bucket, parsed.path().trim_start_matches('/'))
        } else {
            parsed.path().to_string()
        };
        let canonical_query = signing::canonical_query_string(parsed.query_pairs());
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers = format!(
            "host:{host_header}\nx-amz-content-sha256:{body_hash}\nx-amz-date:{amz_date}\n",
        );
        let canonical_request = format!(
            "{}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{body_hash}",
            method,
        );

        let scope = format!("{date_stamp}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            signing::hex_sha256(canonical_request.as_bytes()),
        );
        let signing_key = signing::derive_signing_key(&self.secret_key, &date_stamp, &self.region);
        let signature = {
            use hmac::Mac;
            let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(&signing_key)
                .map_err(|e| CacheError::Backend(format!("hmac: {e}")))?;
            mac.update(string_to_sign.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        };

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key,
        );

        let mut req = self
            .client
            .request(method, url)
            .header("host", &host_header)
            .header("x-amz-date", &amz_date)
            .header("x-amz-content-sha256", &body_hash)
            .header("authorization", &authorization);

        if let Some(b) = body {
            req = req.body(b);
        }
        req.send().await.map_err(|e| CacheError::Backend(e.to_string()))
    }
}
