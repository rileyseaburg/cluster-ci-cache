//! ci-cache CLI: restore/save CI build caches.

mod client;
mod keygen;
mod ops;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use clap::{Parser, Subcommand, ValueEnum};

const DEFAULT_MAX_DECOMPRESS_BYTES: u64 = 40 * 1024 * 1024 * 1024;
const DEFAULT_MAX_ARCHIVE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

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
        /// Maximum decompressed bytes allowed while extracting (supports B, KiB, MiB, GiB, KB, MB, GB)
        #[arg(long, default_value_t = DEFAULT_MAX_DECOMPRESS_BYTES, value_parser = parse_byte_limit)]
        max_decompress_size: u64,
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
        /// Add manifest metadata as key=value. Can be repeated.
        #[arg(long = "metadata", value_parser = parse_key_value)]
        metadata: Vec<(String, String)>,
        /// Maximum archive bytes allowed per saved path (supports B, KiB, MiB, GiB, KB, MB, GB)
        #[arg(long, default_value_t = DEFAULT_MAX_ARCHIVE_BYTES, value_parser = parse_byte_limit)]
        max_archive_size: u64,
    },
    /// Docker/BuildKit cache integration helpers
    Docker {
        #[command(subcommand)]
        sub: DockerCommands,
    },
    /// Deterministic cache key generators
    Key {
        #[command(subcommand)]
        sub: KeyCommands,
    },
    /// Authentication and CI environment helpers
    Auth {
        #[command(subcommand)]
        sub: AuthCommands,
    },
    /// Print a cache manifest as pretty JSON
    Manifest {
        #[arg(long)]
        key: String,
    },
    /// Delete a cache manifest by key
    Delete {
        #[arg(long)]
        key: String,
    },
    /// Check server health
    Health,
}

#[derive(Subcommand)]
enum DockerCommands {
    /// Print instructions for setting up BuildKit registry cache
    LoginCache,
    /// Print cache-from flags for a given key
    CacheFrom {
        #[arg(long)]
        key: String,
    },
    /// Print cache-to flags for a given key
    CacheTo {
        #[arg(long)]
        key: String,
    },
    /// Print just the BuildKit cache ref value for a key
    CacheRef {
        #[arg(value_enum)]
        action: DockerCacheAction,
        #[arg(long)]
        key: String,
    },
    /// Print ready-to-pass BuildKit cache flags for import keys and an export key
    BuildkitFlags {
        /// Cache import key. Can be repeated.
        #[arg(long = "from")]
        from: Vec<String>,
        /// Cache export key.
        #[arg(long)]
        to: String,
        /// Fallback cache-to ref value if needed.
        #[arg(long)]
        fallback_to: Option<String>,
        /// Check import refs with `docker buildx imagetools inspect` before printing them.
        #[arg(long)]
        verify_importers: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum DockerCacheAction {
    CacheFrom,
    CacheTo,
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Print a CI cache token discovered from env, Vault, or Kubernetes Secret
    Token {
        /// Vault KV path containing the token. Also read from CI_CACHE_VAULT_TOKEN_PATH.
        #[arg(long, env = "CI_CACHE_VAULT_TOKEN_PATH")]
        vault_path: Option<String>,
        /// Vault field name containing the token.
        #[arg(
            long,
            env = "CI_CACHE_VAULT_TOKEN_FIELD",
            default_value = "CI_CACHE_TOKEN"
        )]
        vault_field: String,
        /// Kubernetes namespace containing the auth Secret.
        #[arg(long, env = "CI_CACHE_KUBE_NAMESPACE", default_value = "ci-cache")]
        kube_namespace: String,
        /// Kubernetes Secret containing the token.
        #[arg(long, env = "CI_CACHE_KUBE_SECRET", default_value = "ci-cache-auth")]
        kube_secret: String,
        /// Kubernetes Secret data key containing the token.
        #[arg(long, env = "CI_CACHE_KUBE_TOKEN_KEY", default_value = "token")]
        kube_token_key: String,
    },
    /// Print shell exports for CI_CACHE_SERVER, CI_CACHE_NAMESPACE, and CI_CACHE_TOKEN
    Env {
        /// Server URL to export.
        #[arg(
            long,
            env = "CI_CACHE_SERVER",
            default_value = "http://192.168.50.82/ci-cache"
        )]
        server: String,
        /// Namespace to export.
        #[arg(
            long,
            env = "CI_CACHE_NAMESPACE",
            default_value = "spotlessbinco-trusted-v5"
        )]
        namespace: String,
        /// Vault KV path containing the token. Also read from CI_CACHE_VAULT_TOKEN_PATH.
        #[arg(long, env = "CI_CACHE_VAULT_TOKEN_PATH")]
        vault_path: Option<String>,
        /// Vault field name containing the token.
        #[arg(
            long,
            env = "CI_CACHE_VAULT_TOKEN_FIELD",
            default_value = "CI_CACHE_TOKEN"
        )]
        vault_field: String,
        /// Kubernetes namespace containing the auth Secret.
        #[arg(long, env = "CI_CACHE_KUBE_NAMESPACE", default_value = "ci-cache")]
        kube_namespace: String,
        /// Kubernetes Secret containing the token.
        #[arg(long, env = "CI_CACHE_KUBE_SECRET", default_value = "ci-cache-auth")]
        kube_secret: String,
        /// Kubernetes Secret data key containing the token.
        #[arg(long, env = "CI_CACHE_KUBE_TOKEN_KEY", default_value = "token")]
        kube_token_key: String,
    },
}

#[derive(Subcommand)]
enum KeyCommands {
    /// Generate the standard Fastify/API + UI assets cache key
    ApiUiAssets {
        /// Repository root. Defaults to the current directory's git root.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// Git revision to inspect
        #[arg(long, default_value = "HEAD")]
        revision: String,
    },
    /// Generate a key from tracked git tree entries
    GitTree {
        /// Prefix to place before the digest
        #[arg(long)]
        prefix: String,
        /// Repository root. Defaults to the current directory's git root.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// Git revision to inspect
        #[arg(long, default_value = "HEAD")]
        revision: String,
        /// Tracked path to include. Can be repeated.
        #[arg(long = "include", required = true)]
        includes: Vec<String>,
        /// Exact tracked path to exclude after expansion. Can be repeated.
        #[arg(long = "exclude")]
        exclude: Vec<String>,
        /// Tracked path prefix to exclude after expansion. Can be repeated.
        #[arg(long = "exclude-prefix")]
        exclude_prefix: Vec<String>,
        /// Omit OS/architecture from the emitted key.
        #[arg(long)]
        no_platform: bool,
    },
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
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
            max_decompress_size,
        } => {
            let ct = ci_cache_core::CacheType::parse(cache_type)
                .ok_or_else(|| anyhow::anyhow!("unknown cache type: {cache_type}"))?;
            let path_list: Vec<String> = paths.split(',').map(|s| s.trim().to_string()).collect();

            let result = ops::restore(
                &client,
                &cli.namespace,
                ct,
                key,
                &path_list,
                *max_decompress_size,
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
            metadata,
            max_archive_size,
        } => {
            let ct = ci_cache_core::CacheType::parse(cache_type)
                .ok_or_else(|| anyhow::anyhow!("unknown cache type: {cache_type}"))?;
            let path_list: Vec<String> = paths.split(',').map(|s| s.trim().to_string()).collect();

            let result = ops::save(
                &client,
                &cli.namespace,
                ct,
                key,
                &path_list,
                compression,
                *max_archive_size,
                *ttl,
                metadata.iter().cloned().collect::<HashMap<_, _>>(),
            )
            .await?;

            println!(
                "SAVED key={} blobs={} uploaded={} deduped={} metadata={}",
                result.key,
                result.blob_count,
                ci_cache_core::format_bytes(result.bytes_uploaded),
                ci_cache_core::format_bytes(result.bytes_deduped),
                metadata.len(),
            );
        }
        Commands::Manifest { key } => {
            let manifest = client.get_manifest(&cli.namespace, key).await?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Commands::Delete { key } => {
            client.delete_manifest(&cli.namespace, key).await?;
            println!("DELETED key={}", key);
        }
        Commands::Docker { sub } => {
            handle_docker(sub)?;
        }
        Commands::Key { sub } => {
            handle_key(sub)?;
        }
        Commands::Auth { sub } => {
            handle_auth(sub, cli.token.as_deref())?;
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
            println!(
                "docker buildx create --use --name ci-cache-builder \\
  --driver docker-container"
            );
            println!("");
            println!(
                "docker buildx build \\
  --cache-from type=registry,ref=$REGISTRY/cache/$IMAGE:buildcache \\
  --cache-to type=registry,ref=$REGISTRY/cache/$IMAGE:buildcache,mode=max \\
  -t $REGISTRY/$IMAGE:$TAG \\
  --push ."
            );
        }
        DockerCommands::CacheFrom { key } => {
            let reg =
                std::env::var("CI_CACHE_REGISTRY").unwrap_or_else(|_| "registry.local".into());
            println!("--cache-from type=registry,ref={reg}/cache/{key}");
        }
        DockerCommands::CacheTo { key } => {
            let reg =
                std::env::var("CI_CACHE_REGISTRY").unwrap_or_else(|_| "registry.local".into());
            println!("--cache-to type=registry,ref={reg}/cache/{key},mode=max");
        }
        DockerCommands::CacheRef { action, key } => {
            println!("{}", docker_cache_ref(*action, key));
        }
        DockerCommands::BuildkitFlags {
            from,
            to,
            fallback_to,
            verify_importers,
        } => {
            for key in from {
                let cache_ref = docker_cache_ref(DockerCacheAction::CacheFrom, key);
                if !verify_importers || docker_cache_ref_exists(&cache_ref) {
                    println!("--cache-from={cache_ref}");
                } else {
                    eprintln!("ci-cache: no usable BuildKit cache importer for {key}; skipping it");
                }
            }

            let to_ref = docker_cache_ref(DockerCacheAction::CacheTo, to);
            if !to_ref.is_empty() {
                println!("--cache-to={to_ref}");
            } else if let Some(fallback) = fallback_to {
                println!("--cache-to={fallback}");
            }
        }
    }
    Ok(())
}

fn docker_cache_ref(action: DockerCacheAction, key: &str) -> String {
    let reg = std::env::var("CI_CACHE_REGISTRY").unwrap_or_else(|_| "registry.local".into());
    match action {
        DockerCacheAction::CacheFrom => format!("type=registry,ref={reg}/cache/{key}"),
        DockerCacheAction::CacheTo => format!("type=registry,ref={reg}/cache/{key},mode=max"),
    }
}

fn docker_cache_ref_exists(cache_ref: &str) -> bool {
    let Some(image) = cache_ref
        .split(',')
        .find_map(|part| part.strip_prefix("ref="))
    else {
        return false;
    };

    Command::new("docker")
        .args(["buildx", "imagetools", "inspect", image])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn handle_key(sub: &KeyCommands) -> anyhow::Result<()> {
    let key = match sub {
        KeyCommands::ApiUiAssets {
            repo_root,
            revision,
        } => keygen::api_ui_assets_key(repo_root, revision)?,
        KeyCommands::GitTree {
            prefix,
            repo_root,
            revision,
            includes,
            exclude,
            exclude_prefix,
            no_platform,
        } => keygen::git_tree_key(keygen::GitTreeKeyOptions {
            repo_root: repo_root.clone(),
            revision: revision.clone(),
            prefix: prefix.clone(),
            includes: includes.clone(),
            exclude: exclude.clone(),
            exclude_prefix: exclude_prefix.clone(),
            include_platform: !no_platform,
        })?,
    };

    println!("{key}");
    Ok(())
}

fn handle_auth(sub: &AuthCommands, cli_token: Option<&str>) -> anyhow::Result<()> {
    match sub {
        AuthCommands::Token {
            vault_path,
            vault_field,
            kube_namespace,
            kube_secret,
            kube_token_key,
        } => {
            let token = discover_token(
                cli_token,
                vault_path.as_deref(),
                vault_field,
                kube_namespace,
                kube_secret,
                kube_token_key,
            )?;
            println!("{token}");
        }
        AuthCommands::Env {
            server,
            namespace,
            vault_path,
            vault_field,
            kube_namespace,
            kube_secret,
            kube_token_key,
        } => {
            let token = discover_token(
                cli_token,
                vault_path.as_deref(),
                vault_field,
                kube_namespace,
                kube_secret,
                kube_token_key,
            )?;
            println!("export CI_CACHE_SERVER={}", shell_quote(server));
            println!("export CI_CACHE_NAMESPACE={}", shell_quote(namespace));
            println!("export CI_CACHE_TOKEN={}", shell_quote(&token));
        }
    }
    Ok(())
}

fn discover_token(
    cli_token: Option<&str>,
    vault_path: Option<&str>,
    vault_field: &str,
    kube_namespace: &str,
    kube_secret: &str,
    kube_token_key: &str,
) -> anyhow::Result<String> {
    if let Some(token) = cli_token.filter(|token| !token.is_empty()) {
        return Ok(token.to_string());
    }
    if let Some(token) = vault_path.and_then(|path| read_vault_token(path, vault_field)) {
        return Ok(token);
    }
    if let Some(token) = read_kube_secret_token(kube_namespace, kube_secret, kube_token_key) {
        return Ok(token);
    }
    anyhow::bail!(
        "CI cache token is not accessible via CI_CACHE_TOKEN, Vault, or Kubernetes Secret"
    )
}

fn read_vault_token(path: &str, field: &str) -> Option<String> {
    let output = Command::new("vault")
        .args(["kv", "get", "-field", field, path])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| trim_stdout(output.stdout))?
}

fn read_kube_secret_token(namespace: &str, secret: &str, key: &str) -> Option<String> {
    let template = format!("{{{{ index .data {:?} | base64decode }}}}", key);
    let output = Command::new("kubectl")
        .args([
            "-n",
            namespace,
            "get",
            "secret",
            secret,
            "-o",
            &format!("go-template={template}"),
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| trim_stdout(output.stdout))?
}

fn trim_stdout(stdout: Vec<u8>) -> Option<String> {
    let token = String::from_utf8(stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_key_value(input: &str) -> Result<(String, String), String> {
    let (key, value) = input
        .split_once('=')
        .ok_or_else(|| "metadata must be in key=value form".to_string())?;
    let key = key.trim();
    if key.is_empty() {
        return Err("metadata key cannot be empty".to_string());
    }
    Ok((key.to_string(), value.to_string()))
}

fn parse_byte_limit(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("size cannot be empty".to_string());
    }

    let number_end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (number, unit) = s.split_at(number_end);
    if number.is_empty() {
        return Err("size must start with a number".to_string());
    }

    let value: f64 = number
        .parse()
        .map_err(|_| format!("invalid size number: {number}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err("size must be a non-negative finite number".to_string());
    }

    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1_f64,
        "k" | "kb" => 1_000_f64,
        "m" | "mb" => 1_000_000_f64,
        "g" | "gb" => 1_000_000_000_f64,
        "t" | "tb" => 1_000_000_000_000_f64,
        "kib" => 1024_f64,
        "mib" => 1024_f64.powi(2),
        "gib" => 1024_f64.powi(3),
        "tib" => 1024_f64.powi(4),
        other => return Err(format!("unsupported size unit: {other}")),
    };

    let bytes = value * multiplier;
    if bytes > u64::MAX as f64 {
        return Err("size is too large".to_string());
    }
    Ok(bytes as u64)
}

#[cfg(test)]
mod tests {
    use super::{parse_byte_limit, parse_key_value};

    #[test]
    fn parses_metadata_key_value() {
        assert_eq!(
            parse_key_value("commit=abc123").unwrap(),
            ("commit".to_string(), "abc123".to_string())
        );
        assert_eq!(
            parse_key_value("run-id=").unwrap(),
            ("run-id".to_string(), "".to_string())
        );
    }

    #[test]
    fn rejects_bad_metadata() {
        assert!(parse_key_value("missing-separator").is_err());
        assert!(parse_key_value("=value").is_err());
    }

    #[test]
    fn parses_byte_limits() {
        assert_eq!(parse_byte_limit("42").unwrap(), 42);
        assert_eq!(parse_byte_limit("1KB").unwrap(), 1_000);
        assert_eq!(parse_byte_limit("1KiB").unwrap(), 1024);
        assert_eq!(parse_byte_limit("1.5MiB").unwrap(), 1_572_864);
        assert_eq!(parse_byte_limit("2GiB").unwrap(), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn rejects_bad_byte_limits() {
        assert!(parse_byte_limit("").is_err());
        assert!(parse_byte_limit("GiB").is_err());
        assert!(parse_byte_limit("1XB").is_err());
        assert!(parse_byte_limit("-1").is_err());
    }
}
