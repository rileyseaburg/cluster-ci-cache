//! ci-cache-agent: node-local DaemonSet agent.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use clap::Parser;

#[derive(Parser)]
#[command(name = "ci-cache-agent", version, about = "Node-local cache agent")]
struct Args {
    #[arg(long, env = "CI_CACHE_AGENT_ADDR", default_value = "0.0.0.0:8090")]
    addr: String,
    #[arg(long, env = "CI_CACHE_AGENT_CACHE_DIR", default_value = "/var/lib/ci-cache-agent")]
    cache_dir: String,
    #[arg(long, env = "CI_CACHE_SERVER", default_value = "http://ci-cache-server:8080")]
    upstream: String,
    #[arg(long, env = "CI_CACHE_AGENT_MAX_GB", default_value_t = 10)]
    max_gb: u64,
}

struct AgentState {
    cache_dir: String,
    upstream: String,
    max_bytes: u64,
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let args = Args::parse();
    std::fs::create_dir_all(&args.cache_dir)?;
    let state = Arc::new(AgentState {
        cache_dir: args.cache_dir.clone(),
        upstream: args.upstream.clone(),
        max_bytes: args.max_gb * 1024 * 1024 * 1024,
    });
    tracing::info!(addr = %args.addr, cache_dir = %args.cache_dir, "starting ci-cache-agent");

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/agent/status", get(agent_status))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.addr).await?;
    tracing::info!("agent listening on {}", args.addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<Arc<AgentState>>) -> impl IntoResponse {
    let test = std::path::Path::new(&state.cache_dir).join(".readyz");
    match std::fs::write(&test, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&test);
            (StatusCode::OK, "ready".to_string())
        }
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, format!("not ready: {e}")),
    }
}

async fn agent_status(State(state): State<Arc<AgentState>>) -> impl IntoResponse {
    let mut total_size: u64 = 0;
    let mut blob_count: usize = 0;
    if let Ok(entries) = std::fs::read_dir(&state.cache_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total_size += meta.len();
                    blob_count += 1;
                }
            }
        }
    }
    let usage_pct = if state.max_bytes > 0 {
        (total_size as f64 / state.max_bytes as f64) * 100.0
    } else {
        0.0
    };
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "cache_dir": state.cache_dir,
            "upstream": state.upstream,
            "blob_count": blob_count,
            "total_bytes": total_size,
            "max_bytes": state.max_bytes,
            "usage_percent": usage_pct.round(),
            "status": if usage_pct > 90.0 { "critical" } else { "ok" },
        })),
    )
}
