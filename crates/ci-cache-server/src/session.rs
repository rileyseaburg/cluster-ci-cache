//! Save session manager: tracks in-flight save sessions before finalization.
//! Sessions prevent partial/corrupt manifests from being published.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use ci_cache_core::manifest::dto::SaveStartRequest;
use ci_cache_core::manifest::{BlobRef, CachedPath};

/// An in-flight save session.
#[derive(Debug, Clone)]
pub struct SaveSession {
    pub id: String,
    pub namespace: String,
    pub key: String,
    pub cache_type: ci_cache_core::CacheType,
    pub ttl_seconds: Option<u64>,
    pub metadata: HashMap<String, String>,
    pub blobs: Vec<BlobRef>,
    pub bytes_uploaded: u64,
    pub bytes_deduped: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SaveSession {
    pub fn from_start(req: SaveStartRequest, id: String) -> Self {
        Self {
            id,
            namespace: req.namespace,
            key: req.key,
            cache_type: req.cache_type,
            ttl_seconds: req.ttl_seconds,
            metadata: req.metadata,
            blobs: Vec::new(),
            bytes_uploaded: 0,
            bytes_deduped: 0,
            created_at: chrono::Utc::now(),
        }
    }
}

/// Thread-safe session store.
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SaveSession>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, session: SaveSession) {
        let id = session.id.clone();
        self.sessions.write().await.insert(id, session);
    }

    pub async fn get(&self, id: &str) -> Option<SaveSession> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn record_blob(
        &self,
        id: &str,
        blob: BlobRef,
        deduped: bool,
    ) {
        if let Some(s) = self.sessions.write().await.get_mut(id) {
            s.blobs.push(blob.clone());
            if deduped {
                s.bytes_deduped += blob.size_bytes;
            } else {
                s.bytes_uploaded += blob.size_bytes;
            }
        }
    }

    pub async fn remove(&self, id: &str) -> Option<SaveSession> {
        self.sessions.write().await.remove(id)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// DTO for the save-finish request (paths + blobs from client).
pub use ci_cache_core::manifest::dto::{SaveFinishRequest, SaveFinishResponse};
