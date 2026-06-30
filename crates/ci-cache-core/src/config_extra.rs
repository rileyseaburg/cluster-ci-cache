//! Cache behavior, auth, and AppConfig loading.

use serde::{Deserialize, Serialize};

use crate::compression::Compression;
use crate::config::{BackendConfig, ServerConfig};
use crate::error::{CacheError, Result};

/// Cache behavior knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_ttl")]
    pub default_ttl_seconds: u64,
    #[serde(default = "default_compression")]
    pub compression: Compression,
    #[serde(default = "default_max_blob")]
    pub max_blob_size_mb: u64,
    #[serde(default = "default_max_archive")]
    pub max_archive_size_gb: u64,
    #[serde(default = "default_dedupe")]
    pub dedupe: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl_seconds: default_ttl(),
            compression: default_compression(),
            max_blob_size_mb: default_max_blob(),
            max_archive_size_gb: default_max_archive(),
            dedupe: default_dedupe(),
        }
    }
}

fn default_ttl() -> u64 { 604_800 }
fn default_compression() -> Compression { Compression::Zstd }
fn default_max_blob() -> u64 { 512 }
fn default_max_archive() -> u64 { 10 }
fn default_dedupe() -> bool { true }

impl CacheConfig {
    pub fn max_blob_bytes(&self) -> u64 {
        self.max_blob_size_mb * 1024 * 1024
    }
    pub fn max_archive_bytes(&self) -> u64 {
        self.max_archive_size_gb * 1024 * 1024 * 1024
    }
    pub fn max_decompress_bytes(&self) -> u64 {
        self.max_archive_bytes() * 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    None,
    ServiceAccount,
    Token,
}

impl Default for AuthMode {
    fn default() -> Self { AuthMode::ServiceAccount }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: AuthMode,
    #[serde(default = "default_token_env")]
    pub token_env: String,
}

fn default_token_env() -> String {
    "CI_CACHE_AUTH_TOKEN".to_string()
}

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub backend: BackendConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            backend: BackendConfig::default(),
            cache: CacheConfig::default(),
            auth: AuthConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load_file(path: &str) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| CacheError::Config(format!("read config {path}: {e}")))?;
        Self::load_str(&raw)
    }

    pub fn load_str(raw: &str) -> Result<Self> {
        serde_yaml::from_str(raw)
            .map_err(|e| CacheError::Config(format!("parse config: {e}")))
    }

    pub fn from_env_or_default() -> Self {
        if let Ok(path) = std::env::var("CI_CACHE_CONFIG") {
            match Self::load_file(&path) {
                Ok(cfg) => return cfg,
                Err(e) => tracing::warn!("config load {path}: {e}; defaults"),
            }
        }
        AppConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendKind;

    #[test]
    fn test_parse_yaml() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:9090"
backend:
  kind: "s3"
  bucket: "test"
cache:
  compression: "gzip"
"#;
        let cfg = AppConfig::load_str(yaml).unwrap();
        assert_eq!(cfg.server.listen_addr, "0.0.0.0:9090");
        assert_eq!(cfg.backend.kind, BackendKind::S3);
        assert_eq!(cfg.cache.compression, Compression::Gzip);
    }

    #[test]
    fn test_defaults() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.backend.kind, BackendKind::Local);
        assert_eq!(cfg.cache.compression, Compression::Zstd);
        assert!(cfg.cache.dedupe);
    }
}
