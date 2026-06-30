//! ci-cache CLI: restore/save CI build caches.

mod client;
mod ops;

use std::collections::HashMap;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ci-cache", version, about = "Cluster CI Cache CLI")]
struct Cli {
    /// Server base URL (e.g. http://ci-cache-server:8080)
    #[arg(long, env = "CI_CACHE_SERVER", default_value = "http://localhost:8080")]
    server: String,

    /// Namespace (team/project scope)
    #[arg(long, env = "CI_CACHE_NAMESPACE", default_value = "default")]
    namespace: String,

    /// Bearer auth token
    #[arg(long, env = "CI_CACHE_TOKEN")]
    token: Option<String>,

    /// Compression algorithm
    #[arg(long, env = "CI_CACHE_COMPRESSION", default_value = "zstd")]
    compression: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Restore a cache entry
    Restore {
        #[arg(long)]
        cache_type: String,
        #[arg(long)]
        key: String,
        /// Comma-separated paths to restore
        #[arg(long)]
        paths: String,
    },
    /// Save a cache entry
    Save {
        #[arg(long)]
        cache_type: String,
        #[arg(long)]
        key: String,
        /// Comma-separated paths to save
        #[arg(long)]
        paths: String,
        /// TTL in seconds
        #[arg(long)]
        ttl: Option<u64>,
    },
    /// Docker/BuildKit cache integration helpers
    Docker {
        #[command(subcommand)]
        sub: DockerCommands,
    },
    /// Check server health
    Health,
}

#[derive(Subcommand)]
enum DockerCommands {
    /// Print instructions for setting up BuildKit registry cache
    LoginCache,
    /// Print cache-from flags for a given key
    CacheFrom { #[arg(long)] key: String },
    /// Print cache-to flags for a given key
    CacheTo { #[arg(long)] key: String },
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let compression = ci_cache_core::Compression::parse(&cli.compression)
        .map_err(|e| anyhow::anyhow!("invalid compression: {e}"))?;

    let client = client::CacheClient::new(&cli.server, cli.token.clone())?;

    match &cli.command {
        Commands::Health => {
            let ok = client.health().await?;
            if ok {
                println!("healthy");
            } else {
                anyhow::bail!("server unhealthy");
            }
        }
        Commands::Restore {
            cache_type,
            key,
            paths,
        } => {
            let ct = ci_cache_core::CacheType::parse(cache_type)
                .ok_or_else(|| anyhow::anyhow!("unknown cache type: {cache_type}"))?;
            let path_list: Vec<String> =
                paths.split(',').map(|s| s.trim().to_string()).collect();

            let result = ops::restore(
                &client,
                &cli.namespace,
                ct,
                key,
                &path_list,
                40 * 1024 * 1024 * 1024, // 40 GiB decompress limit
            )
            .await?;

            if result.hit {
                println!(
                    "RESTORED key={} files={} bytes={}",
                    key,
                    result.files_restored,
                    ci_cache_core::format_bytes(result.bytes_downloaded)
                );
            } else {
                println!("MISS key={}", key);
            }
        }
        Commands::Save {
            cache_type,
            key,
            paths,
            ttl,
        } => {
            let ct = ci_cache_core::CacheType::parse(cache_type)
                .ok_or_else(|| anyhow::anyhow!("unknown cache type: {cache_type}"))?;
            let path_list: Vec<String> =
                paths.split(',').map(|s| s.trim().to_string()).collect();

            let result = ops::save(
                &client,
                &cli.namespace,
                ct,
                key,
                &path_list,
                compression,
                10 * 1024 * 1024 * 1024, // 10 GiB max archive
                *ttl,
            )
            .await?;

            println!(
                "SAVED key={} blobs={} uploaded={} deduped={}",
                result.key,
                result.blob_count,
                ci_cache_core::format_bytes(result.bytes_uploaded),
                ci_cache_core::format_bytes(result.bytes_deduped),
            );
        }
        Commands::Docker { sub } => {
            handle_docker(sub)?;
        }
    }

    Ok(())
}

fn handle_docker(sub: &DockerCommands) -> anyhow::Result<()> {
    match sub {
        DockerCommands::LoginCache => {
            println!("# BuildKit registry cache setup");
            println!("# Set BUILDKIT_INLINE_CACHE=1 in your Dockerfile for inline cache");
            println!("# For registry-type cache, use these buildx flags:");
            println!("docker buildx create --use --name ci-cache-builder \\
  --driver docker-container");
            println!("");
            println!("docker buildx build \\
  --cache-from type=registry,ref=$REGISTRY/cache/$IMAGE:buildcache \\
  --cache-to type=registry,ref=$REGISTRY/cache/$IMAGE:buildcache,mode=max \\
  -t $REGISTRY/$IMAGE:$TAG \\
  --push .");
        }
        DockerCommands::CacheFrom { key } => {
            let reg = std::env::var("CI_CACHE_REGISTRY").unwrap_or_else(|_| "registry.local".into());
            println!("--cache-from type=registry,ref={reg}/cache/{key}");
        }
        DockerCommands::CacheTo { key } => {
            let reg = std::env::var("CI_CACHE_REGISTRY").unwrap_or_else(|_| "registry.local".into());
            println!("--cache-to type=registry,ref={reg}/cache/{key},mode=max");
        }
    }
    Ok(())
}
