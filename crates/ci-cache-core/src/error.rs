//! Error types for Cluster CI Cache.

/// A specialized error type for cache operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("blob not found: {0}")]
    BlobNotFound(String),

    #[error("manifest not found for namespace={namespace} key={key}")]
    ManifestNotFound { namespace: String, key: String },

    #[error("digest mismatch: expected={expected} actual={actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("blob too large: {size} bytes exceeds limit of {limit} bytes")]
    BlobTooLarge { size: u64, limit: u64 },

    #[error("archive too large: exceeds limit of {limit} bytes")]
    ArchiveTooLarge { limit: u64 },

    #[error("decompression bomb detected: decompressed size {size} exceeds limit {limit}")]
    DecompressionBomb { size: u64, limit: u64 },

    #[error("path traversal detected: {0}")]
    PathTraversal(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("compression error: {0}")]
    Compression(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("{0}")]
    Other(String),
}

impl CacheError {
    /// Wrap an arbitrary error message.
    pub fn other(msg: impl Into<String>) -> Self {
        CacheError::Other(msg.into())
    }

    /// Wrap an arbitrary error message as a backend error.
    pub fn backend(msg: impl Into<String>) -> Self {
        CacheError::Backend(msg.into())
    }

    /// Whether this error indicates a missing cache entry (a cache miss),
    /// as opposed to a real failure.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            CacheError::BlobNotFound(_)
                | CacheError::ManifestNotFound { .. }
                | CacheError::NotFound(_)
        )
    }
}

/// Convenience Result alias.
pub type Result<T> = std::result::Result<T, CacheError>;
