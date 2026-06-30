//! Core models, traits, and utilities for Cluster CI Cache.

pub mod archive;
pub mod archive_ops;
pub mod backend;
pub mod compression;
pub mod config;
pub mod config_extra;
pub mod digest;
pub mod error;
pub mod manifest;
pub mod metrics;
pub mod paths;

// Re-exports
pub use archive::{create_archive_from_dir, ArchiveEntry, ArchiveHeader};
pub use archive_ops::extract_archive;
pub use backend::{BlobLocation, ByteStream, CacheBackend};
pub use compression::Compression;
pub use config::{BackendKind, BackendConfig, ServerConfig};
pub use config_extra::{AppConfig, AuthConfig, AuthMode, CacheConfig};
pub use error::{CacheError, Result};
pub use manifest::{BlobRef, CacheManifest, CacheType, CachedPath};

use sha2::{Digest, Sha256};

/// Compute the sha256 digest of a byte slice, returning `sha256:<hex>`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Incremental streaming hasher.
pub struct StreamingDigest {
    inner: Sha256,
}

impl StreamingDigest {
    pub fn new() -> Self {
        Self { inner: Sha256::new() }
    }
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }
    pub fn finalize(self) -> String {
        format!("sha256:{}", hex::encode(self.inner.finalize()))
    }
}

impl Default for StreamingDigest {
    fn default() -> Self { Self::new() }
}

/// Format a byte count as a human readable string.
pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.2} KB", n as f64 / KB as f64)
    } else {
        format!("{} B", n)
    }
}
