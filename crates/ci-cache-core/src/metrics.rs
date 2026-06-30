//! Prometheus metrics registry for the cache server.

use prometheus::{IntCounter, Histogram, HistogramOpts, Opts, Registry};

/// Holds all Prometheus metrics for the cache service.
#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub restore_requests: IntCounter,
    pub save_requests: IntCounter,
    pub hits: IntCounter,
    pub misses: IntCounter,
    pub bytes_uploaded: IntCounter,
    pub bytes_downloaded: IntCounter,
    pub blob_dedup_hits: IntCounter,
    pub backend_errors: IntCounter,
    pub restore_duration: Histogram,
    pub save_duration: Histogram,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let mk = |name: &str, help: &str| {
            IntCounter::with_opts(Opts::new(name, help)).unwrap()
        };
        let restore_requests = mk(
            "ci_cache_restore_requests_total",
            "Total cache restore requests",
        );
        let save_requests = mk(
            "ci_cache_save_requests_total",
            "Total cache save requests",
        );
        let hits = mk("ci_cache_hits_total", "Total cache hits");
        let misses = mk("ci_cache_misses_total", "Total cache misses");
        let bytes_uploaded = mk(
            "ci_cache_bytes_uploaded_total",
            "Total bytes uploaded",
        );
        let bytes_downloaded = mk(
            "ci_cache_bytes_downloaded_total",
            "Total bytes downloaded",
        );
        let blob_dedup_hits = mk(
            "ci_cache_blob_dedup_hits_total",
            "Total blob dedup hits (skipped uploads)",
        );
        let backend_errors = mk(
            "ci_cache_backend_errors_total",
            "Total backend errors",
        );

        let restore_duration = Histogram::with_opts(HistogramOpts::new(
            "ci_cache_restore_duration_seconds",
            "Cache restore duration in seconds",
        ))
        .unwrap();
        let save_duration = Histogram::with_opts(HistogramOpts::new(
            "ci_cache_save_duration_seconds",
            "Cache save duration in seconds",
        ))
        .unwrap();

        for m in [
            &restore_requests, &save_requests, &hits, &misses,
            &bytes_uploaded, &bytes_downloaded, &blob_dedup_hits,
            &backend_errors,
        ] {
            registry.register(Box::new(m.clone())).ok();
        }
        registry.register(Box::new(restore_duration.clone())).ok();
        registry.register(Box::new(save_duration.clone())).ok();

        Self {
            registry,
            restore_requests,
            save_requests,
            hits,
            misses,
            bytes_uploaded,
            bytes_downloaded,
            blob_dedup_hits,
            backend_errors,
            restore_duration,
            save_duration,
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
