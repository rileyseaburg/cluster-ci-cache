//! S3-compatible backend for Cluster CI Cache.
mod signing;
pub mod backend;
mod impl_cache;
pub use backend::S3Backend;
