//! Backend factory: creates the right CacheBackend from AppConfig.

use ci_cache_core::backend::CacheBackend;
use ci_cache_core::config_extra::AppConfig;
use ci_cache_core::error::{CacheError, Result};

use ci_cache_backend_fs::FsBackend;
use ci_cache_backend_s3::S3Backend;

/// Create a backend trait object from configuration.
pub fn create_backend(cfg: &AppConfig) -> Result<Box<dyn CacheBackend>> {
    match cfg.backend.kind {
        ci_cache_core::BackendKind::Local => {
            tracing::info!(root = ?cfg.backend.fs_root, "using local filesystem backend");
            let backend = FsBackend::new(&cfg.backend.fs_root)?;
            Ok(Box::new(backend))
        }
        ci_cache_core::BackendKind::S3 => {
            let access_key = std::env::var(&cfg.backend.access_key_env).map_err(|_| {
                CacheError::Config(format!(
                    "missing env var {} for S3 access key",
                    cfg.backend.access_key_env
                ))
            })?;
            let secret_key = std::env::var(&cfg.backend.secret_key_env).map_err(|_| {
                CacheError::Config(format!(
                    "missing env var {} for S3 secret key",
                    cfg.backend.secret_key_env
                ))
            })?;
            tracing::info!(
                endpoint = %cfg.backend.endpoint,
                bucket = %cfg.backend.bucket,
                "using S3 backend"
            );
            let backend = S3Backend::new(
                &cfg.backend.endpoint,
                &cfg.backend.bucket,
                &cfg.backend.region,
                access_key,
                secret_key,
                cfg.backend.path_style,
            )?;
            Ok(Box::new(backend))
        }
    }
}
