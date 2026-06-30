//! Configuration: server and backend settings.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_max_body")]
    pub max_body_bytes: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            max_body_bytes: default_max_body(),
        }
    }
}

fn default_listen_addr() -> String { "0.0.0.0:8080".to_string() }
fn default_max_body() -> u64 { 536_870_912 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    #[serde(default)]
    pub kind: BackendKind,
    #[serde(default = "default_fs_root")]
    pub fs_root: PathBuf,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_access_key_env")]
    pub access_key_env: String,
    #[serde(default = "default_secret_key_env")]
    pub secret_key_env: String,
    #[serde(default = "default_path_style")]
    pub path_style: bool,
}

fn default_fs_root() -> PathBuf { PathBuf::from("/var/lib/ci-cache") }

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            kind: BackendKind::default(),
            fs_root: default_fs_root(),
            bucket: String::new(),
            endpoint: String::new(),
            region: default_region(),
            access_key_env: default_access_key_env(),
            secret_key_env: default_secret_key_env(),
            path_style: default_path_style(),
        }
    }
}

fn default_region() -> String { "us-east-1".to_string() }
fn default_access_key_env() -> String { "CI_CACHE_S3_ACCESS_KEY".to_string() }
fn default_secret_key_env() -> String { "CI_CACHE_S3_SECRET_KEY".to_string() }
fn default_path_style() -> bool { true }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind { Local, S3 }

impl Default for BackendKind {
    fn default() -> Self { BackendKind::Local }
}
