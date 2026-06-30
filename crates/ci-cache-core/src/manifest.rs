//! Manifest and blob data model for Cluster CI Cache.
//!
//! The manifest is the central unit of a cache entry. It references a set of
//! content-addressed blobs stored in a backend, plus metadata describing how
//! those blobs map back to the original filesystem paths.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of build cache. Used for organization and validation but does not
/// change the transport — every cache type is a manifest + blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheType {
    Cargo,
    Npm,
    Pnpm,
    Yarn,
    Docker,
    Generic,
}

impl CacheType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheType::Cargo => "cargo",
            CacheType::Npm => "npm",
            CacheType::Pnpm => "pnpm",
            CacheType::Yarn => "yarn",
            CacheType::Docker => "docker",
            CacheType::Generic => "generic",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cargo" => Some(CacheType::Cargo),
            "npm" => Some(CacheType::Npm),
            "pnpm" => Some(CacheType::Pnpm),
            "yarn" => Some(CacheType::Yarn),
            "docker" => Some(CacheType::Docker),
            "generic" => Some(CacheType::Generic),
            _ => None,
        }
    }
}

impl std::fmt::Display for CacheType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for CacheType {
    fn default() -> Self {
        CacheType::Generic
    }
}

/// A reference to a single content-addressed blob stored in a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRef {
    /// Content digest, e.g. `sha256:<hex>`.
    pub digest: String,
    /// Size of the (possibly compressed) blob in bytes.
    pub size_bytes: u64,
    /// Uncompressed size, useful for accounting.
    pub uncompressed_size_bytes: u64,
    /// Compression algorithm applied to the blob.
    pub compression: String,
    /// Backend name (local, s3, minio, oci, pvc).
    pub backend: String,
    /// Backend-specific location (e.g. key in bucket, path on disk).
    pub location: String,
}

/// Maps an original filesystem path to the archive/blob that backs it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPath {
    /// The path as requested by the user (e.g. `~/.cargo/registry`).
    pub original_path: String,
    /// The archive blob digest that contains this path's contents.
    pub archive_digest: String,
    /// File mode to restore (optional).
    #[serde(default)]
    pub mode: Option<u32>,
}

/// The central cache manifest. One manifest per (namespace, key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    pub namespace: String,
    pub cache_type: CacheType,
    pub key: String,
    /// Schema/format version of the manifest itself.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    /// Paths covered by this manifest, mapped to their archive blobs.
    pub paths: Vec<CachedPath>,
    /// All blobs referenced by this manifest (deduplicated).
    pub blobs: Vec<BlobRef>,
    /// Arbitrary metadata (CI run id, commit, etc.).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl CacheManifest {
    pub const SCHEMA_VERSION: &'static str = "1";

    /// Create a new empty manifest.
    pub fn new(namespace: impl Into<String>, cache_type: CacheType, key: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            namespace: namespace.into(),
            cache_type,
            key: key.into(),
            version: Self::SCHEMA_VERSION.to_string(),
            created_at: now,
            updated_at: now,
            ttl_seconds: None,
            paths: Vec::new(),
            blobs: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Find a blob reference by digest.
    pub fn find_blob(&self, digest: &str) -> Option<&BlobRef> {
        self.blobs.iter().find(|b| b.digest == digest)
    }

    /// Total compressed size of all blobs.
    pub fn total_compressed_size(&self) -> u64 {
        self.blobs.iter().map(|b| b.size_bytes).sum()
    }

    /// Total uncompressed size of all blobs.
    pub fn total_uncompressed_size(&self) -> u64 {
        self.blobs.iter().map(|b| b.uncompressed_size_bytes).sum()
    }

    /// Whether this manifest has expired according to its TTL and the given
    /// reference time.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.ttl_seconds {
            Some(ttl) => {
                let elapsed = now.signed_duration_since(self.updated_at);
                elapsed.num_seconds() > ttl as i64
            }
            None => false,
        }
    }
}

/// Request/response DTOs shared between CLI and server.
pub mod dto {
    use super::{CacheManifest, CacheType};
    use serde::{Deserialize, Serialize};

    /// Restore a cache entry.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RestoreRequest {
        pub namespace: String,
        pub cache_type: CacheType,
        pub key: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RestoreResponse {
        pub hit: bool,
        pub manifest: Option<CacheManifest>,
    }

    /// Start a save session.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SaveStartRequest {
        pub namespace: String,
        pub cache_type: CacheType,
        pub key: String,
        pub ttl_seconds: Option<u64>,
        #[serde(default)]
        pub metadata: std::collections::HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SaveStartResponse {
        pub session_id: String,
    }

    /// Upload a single blob.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlobUploadRequest {
        pub session_id: String,
        pub digest: String,
        pub compression: String,
        #[serde(default)]
        pub metadata: std::collections::HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlobUploadResponse {
        pub stored: bool,
        pub already_present: bool,
        pub digest: String,
        pub size_bytes: u64,
    }

    /// Finalize a save session, publishing the manifest atomically.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SaveFinishRequest {
        pub session_id: String,
        pub paths: Vec<crate::manifest::CachedPath>,
        pub blobs: Vec<crate::manifest::BlobRef>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SaveFinishResponse {
        pub key: String,
        pub bytes_uploaded: u64,
        pub bytes_deduped: u64,
        pub blob_count: usize,
    }

    /// Check whether a blob already exists (for dedup).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HasBlobRequest {
        pub digest: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HasBlobResponse {
        pub present: bool,
    }

    /// Check multiple blobs at once (batch dedup check).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BatchHasBlobsRequest {
        pub digests: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BatchHasBlobsResponse {
        pub present: Vec<String>,
        pub absent: Vec<String>,
    }
}
