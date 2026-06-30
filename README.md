# Cluster CI Cache

**In-cluster, cache-agnostic CI build cache for Kubernetes.**

Cluster CI Cache is a Kubernetes-native artifact/cache fabric that lets CI jobs
restore and save build caches using a consistent interface — regardless of the
underlying storage backend. It supports Rust/Cargo, npm/pnpm/yarn, and
Docker/BuildKit caches.

## Features

- 🚀 **Fast restores** — content-addressed storage with zstd compression and dedup
- 🔒 **Secure** — path-traversal protection, digest verification, decompression bomb prevention
- ☸️ **Kubernetes-native** — Deployment + DaemonSet + Service + RBAC + PVC
- 📦 **Backend-agnostic** — filesystem/PVC and S3/MinIO backends
- 📊 **Observable** — Prometheus metrics, structured JSON logs, health/readiness probes
- 🧩 **Cache types** — Cargo, npm, pnpm, yarn, Docker/BuildKit, generic

## Quick Start

### Deploy to Kubernetes

```bash
# PVC backend (no external dependencies)
kubectl apply -k deploy/k8s/

# Verify
kubectl get pods -l app.kubernetes.io/name=cluster-ci-cache
```

### Use in CI

```bash
# Install the CLI
curl -sSL https://github.com/spotlessbinco/cluster-ci-cache/releases/latest/download/ci-cache-linux-amd64 \
  -o /usr/local/bin/ci-cache && chmod +x /usr/local/bin/ci-cache

# Restore Cargo cache
export CI_CACHE_SERVER=http://ci-cache-server:8080
LOCK_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)
ci-cache restore --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"

# Build
cargo build --release

# Save cache
ci-cache save --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
```

## Build from Source

```bash
cargo build --release
# Binaries: target/release/{ci-cache-server,ci-cache,ci-cache-agent}
```

### Run Tests

```bash
cargo test
```

## Architecture

```
┌──────────────┐     ┌──────────────────────────┐     ┌─────────────┐
│  CI Job Pod  │────▶│   ci-cache-server (HTTP) │────▶│  Backend    │
│  ci-cache    │     │   manifest + blob mgmt    │     │  FS / S3    │
│  (CLI)       │◀────│   dedup + TTL + metrics   │◀────│  PVC / MinIO│
└──────────────┘     └──────────────────────────┘     └─────────────┘
                            │
                     ┌──────┴──────┐
                     │ ci-cache-   │
                     │ agent       │
                     │ (DaemonSet) │
                     └─────────────┘
```

See [docs/architecture.md](docs/architecture.md) for details.

## Configuration

| Setting | Env | Default | Description |
|---------|-----|---------|-------------|
| Server URL | `CI_CACHE_SERVER` | `http://localhost:8080` | Server endpoint |
| Namespace | `CI_CACHE_NAMESPACE` | `default` | Cache namespace |
| Auth token | `CI_CACHE_TOKEN` | — | Bearer token |
| Compression | `CI_CACHE_COMPRESSION` | `zstd` | Compression algorithm |
| Config file | `CI_CACHE_CONFIG` | — | Path to YAML config |

Full config reference: [examples/config.example.yaml](examples/config.example.yaml)

## CLI Usage

```bash
# Restore any cache type
ci-cache restore --cache-type <cargo|npm|pnpm|yarn|docker|generic> \
  --key <key> --paths <comma,separated,paths>

# Save any cache type
ci-cache save --cache-type <type> --key <key> --paths <paths> [--ttl 604800]

# Docker/BuildKit helpers
ci-cache docker login-cache
ci-cache docker cache-from --key <key>
ci-cache docker cache-to --key <key>

# Health check
ci-cache health
```

## Server API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/cache/restore` | Restore a manifest |
| POST | `/v1/cache/save/start` | Start a save session |
| PUT | `/v1/cache/save/blob` | Upload a blob |
| POST | `/v1/cache/save/finish` | Finalize and publish manifest |
| GET | `/v1/cache/manifest/:ns/:key` | Get manifest |
| DELETE | `/v1/cache/manifest/:ns/:key` | Delete manifest |
| GET | `/v1/cache/blob/:digest` | Download a blob |
| DELETE | `/v1/cache/blob/:digest` | Delete a blob |
| POST | `/v1/cache/has-blobs` | Batch blob existence check |
| GET | `/healthz` | Liveness |
| GET | `/readyz` | Readiness |
| GET | `/metrics` | Prometheus metrics |

## Repository Layout

```
cluster-ci-cache/
├── crates/
│   ├── ci-cache-core/        # Models, traits, archive, compression, digest
│   ├── ci-cache-server/      # HTTP API server
│   ├── ci-cache-cli/         # CLI (ci-cache restore/save/docker)
│   ├── ci-cache-agent/       # Node-local DaemonSet agent
│   ├── ci-cache-backend-fs/  # Filesystem/PVC backend
│   └── ci-cache-backend-s3/  # S3/MinIO backend
├── deploy/
│   ├── helm/                 # Helm chart
│   └── k8s/                  # Kustomize manifests
├── examples/                 # CI pipeline examples
└── docs/                     # Architecture, security, operations docs
```

## License

MIT OR Apache-2.0
