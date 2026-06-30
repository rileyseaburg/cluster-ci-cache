<div align="center">

# 🚀 Cluster CI Cache

**In-cluster, cache-agnostic CI build cache for Kubernetes.**

Fast, secure, deduplicated build caching for Rust, npm/pnpm/yarn, and Docker/BuildKit workloads — native to Kubernetes, boring to operate.

![Rust](https://img.shields.io/badge/Rust-1.95-dea584?logo=rust)
![Kubernetes](https://img.shields.io/badge/Kubernetes-1.30+-326ce5?logo=kubernetes)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Status](https://img.shields.io/badge/status-MVP%20Deployed-success)
![Tests](https://img.shields.io/badge/tests-13%20passing-brightgreen)
![Backend](https://img.shields.io/badge/backend-FS%20%7C%20S3%20%7C%20MinIO-orange)
![Compression](https://img.shields.io/badge/compression-zstd-informational)

</div>

---

## 📋 Table of Contents

| Section | Description |
|---------|-------------|
| [What is This?](#-what-is-this) | Problem & solution overview |
| [Why This Works](#-why-this-works) | Design principles |
| [Architecture](#-architecture) | System diagram & components |
| [Data Flow Diagrams](#-data-flow-diagrams) | Save & restore sequences |
| [Quick Start](#-quick-start) | Get running in 2 minutes |
| [Installation](#-installation) | Build from source |
| [Cache Types](#-cache-types) | Cargo, npm, pnpm, yarn, Docker |
| [CLI Reference](#-cli-reference) | Full command docs |
| [Server API](#-server-api) | REST endpoints |
| [Configuration](#-configuration) | YAML & env vars |
| [Kubernetes Deployment](#-kubernetes-deployment) | Manifests & Helm |
| [Observability](#-observability) | Metrics & logging |
| [Security](#-security) | Threat model & mitigations |
| [How Dedup Works](#-how-deduplication-works) | Content addressing explained |
| [Comparison](#-comparison) | vs. other cache solutions |
| [Development](#-development) | Building & testing |
| [Roadmap](#-roadmap) | What's next |

---

## 🤔 What is This?

Cluster CI Cache is a Kubernetes-native build cache system. It lets CI jobs
running inside your cluster restore and save build caches (Cargo, npm, pnpm,
yarn, Docker) through a single interface — whether your storage backend is a
PVC, MinIO, or AWS S3.

### The Problem It Solves

```
❌ Without an in-cluster cache:
  CI pod starts cold → download 500 crates (45s) → compile from scratch (3m)
  Total: 5 minutes per build, repeated 100x/day = 500 min of wasted CI

✅ With Cluster CI Cache:
  CI pod starts → ci-cache restore 0.8s (in-cluster, deduped, zstd)
  → cargo build with warm cache (12s)
  Total: 20 seconds per build
```

---

## 🧠 Why This Works

Three design principles make this fast, safe, and operationally boring:

### 1. Content-Addressed Storage (Not Key-Value)

Every blob is stored under its sha256 digest. Manifests map cache keys to
blob digests. Identical content across different keys is stored **once**:

```
❌  Traditional KV cache:
   key: "cargo-linux-abc"  →  [entire 2GB tarball]
   key: "cargo-linux-def"  →  [entire 2GB tarball]  ← 99% same content!

✅  Content-addressed (our approach):
   key: "cargo-linux-abc"  →  manifest → [blob:A, blob:B, blob:C]
   key: "cargo-linux-def"  →  manifest → [blob:A, blob:D, blob:C]
   blob:A stored ONCE on disk, referenced by both manifests
```

### 2. Manifest-Based Model

Each cache entry is a structured manifest, not an opaque tarball:

```json
{
  "namespace": "team-frontend",
  "cache_type": "cargo",
  "key": "cargo-linux-amd64-a1b2c3",
  "paths": [{ "original_path": "~/.cargo/registry", "archive_digest": "sha256:abc..." }],
  "blobs": [{ "digest": "sha256:abc...", "size_bytes": 45000000, "compression": "zstd" }],
  "ttl_seconds": 604800
}
```

### 3. Security-First Custom Archive Format

No `tar` — tar extraction is a notorious path-traversal vector. We use a
custom `CIC1` format with a JSON header declaring all paths/sizes upfront,
symlink skipping, and decompression bomb protection.

---

## 🏗 Architecture

```
                         Kubernetes Cluster
 ┌─────────────────────────────────────────────────────────────────┐
 │                                                                 │
 │   ┌─────────────┐    ① RESTORE (manifest lookup)               │
 │   │  CI Job Pod │──────────────────────┐                       │
 │   │             │    ④ BLOB DOWNLOAD   │                       │
 │   │  ci-cache   │◀──────────────┐      │                       │
 │   │  (CLI)      │               │      ▼                       │
 │   └─────────────┘               │  ┌──────────────────────┐    │
 │                                 └──│  ci-cache-server     │    │
 │                                    │  (2x replicas)       │    │
 │                                    │                      │    │
 │                                    │  • Manifest mgmt     │    │
 │                                    │  • Blob dedup        │    │
 │                                    │  • TTL / retention   │    │
 │                                    │  • Auth & validation │    │
 │                                    │  • Prometheus metrics│    │
 │                                    └─────────┬────────────┘    │
 │                                              │                 │
 │                                    ┌─────────▼────────────┐    │
 │                                    │   Storage Backend    │    │
 │                                    │  ┌───────┐ ┌───────┐ │    │
 │                                    │  │ PVC   │ │  S3   │ │    │
 │                                    │  │ / FS  │ │ MinIO │ │    │
 │                                    │  └───────┘ └───────┘ │    │
 │                                    └──────────────────────┘    │
 │                                                                 │
 │   ┌─────────────────────────────────────────────────────────┐   │
 │   │  ci-cache-agent (DaemonSet — one per node)              │   │
 │   │  • Node-local blob cache (future)                       │   │
 │   │  • Prefetching / warming (future)                       │   │
 │   │  • Health & status endpoint                             │   │
 │   └─────────────────────────────────────────────────────────┘   │
 └─────────────────────────────────────────────────────────────────┘
```

### Components

| Component | Type | Role |
|-----------|------|------|
| `ci-cache-server` | Deployment (2x) | Central HTTP API, manifest/blob management |
| `ci-cache-cli` | Binary (in CI pods) | Client for restore/save operations |
| `ci-cache-agent` | DaemonSet | Node-local cache, health/status endpoint |

---

## 📊 Data Flow Diagrams

### Restore Flow

```
CI Job                    ci-cache-server              Backend
  │                             │                         │
  │── POST /v1/cache/restore ──▶│                         │
  │   {namespace, type, key}    │                         │
  │                             │── get_manifest(ns,key)─▶│
  │                             │◀──── manifest ──────────│
  │◀── 200 {hit: true,    ──────│                         │
  │       manifest}              │                         │
  │                             │                         │
  │── GET /v1/cache/blob/digest▶│                         │
  │                             │── get_blob(digest) ────▶│
  │                             │◀──── bytes ─────────────│
  │◀──── blob bytes ────────────│                         │
  │                             │                         │
  │  ✓ verify sha256 digest     │                         │
  │  ✓ decompress (size-capped) │                         │
  │  ✓ extract to paths         │                         │
  │    (traversal-safe)          │                         │
  │                             │                         │
  │── RESTORED key=... files=N ─│                         │
```

### Save Flow

```
CI Job                    ci-cache-server              Backend
  │                             │                         │
  │── POST /save/start ────────▶│                         │
  │   {namespace, type, key}    │── creates session ──▶   │
  │◀── {session_id} ────────────│                         │
  │                             │                         │
  │  📁 scan paths               │                         │
  │  📦 create zstd archive     │                         │
  │  🔐 compute sha256 digest   │                         │
  │                             │                         │
  │── PUT /save/blob ──────────▶│                         │
  │   {session, digest, body}   │── verify digest ───▶    │
  │                             │── has_blob? ──────────▶ │
  │                             │◀── yes/no ───────────── │
  │                             │  [if no] put_blob ────▶ │
  │◀── {stored/deduped} ────────│                         │
  │                             │                         │
  │── POST /save/finish ───────▶│                         │
  │   {paths, blobs}            │── put_manifest ──────▶  │
  │                             │   (atomic rename)       │
  │◀── {key, bytes, deduped} ───│                         │
  │                             │                         │
  │── SAVED key=... blobs=N ────│                         │
```

---

## ⚡ Quick Start

### 1. Deploy to Kubernetes

```bash
git clone https://github.com/spotlessbinco/cluster-ci-cache.git
cd cluster-ci-cache

# Deploy with kustomize (uses filesystem/PVC backend)
kubectl apply -k deploy/k8s/

# Or with Helm
helm install cluster-ci-cache deploy/helm/cluster-ci-cache
```

### 2. Verify It's Running

```bash
kubectl get pods -n ci-cache
# NAME                               READY   STATUS    AGE
# ci-cache-server-xxx-aaa            1/1     Running   30s
# ci-cache-server-xxx-bbb            1/1     Running   30s
# ci-cache-agent-xxx                 1/1     Running   30s  (one per node)
```

### 3. Use in Your CI Job

```bash
# Point the CLI at the in-cluster server
export CI_CACHE_SERVER=http://ci-cache-server.ci-cache.svc.cluster.local:8080

# Restore Cargo cache
LOCK_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)
ci-cache restore \
  --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"

# Build
cargo build --release

# Save cache
ci-cache save \
  --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
```

### 4. Watch the Magic

```bash
# First run: MISS → SAVE
ci-cache restore --cache-type cargo --key cargo-abc --paths ~/.cargo/registry
# → MISS key=cargo-abc

ci-cache save --cache-type cargo --key cargo-abc --paths ~/.cargo/registry
# → SAVED key=cargo-abc blobs=1 uploaded=45.2 MB deduped=0 B

# Second run: HIT (instant restore)
ci-cache restore --cache-type cargo --key cargo-abc --paths ~/.cargo/registry
# → RESTORED key=cargo-abc files=2,341 bytes=45.2 MB

# Third run with different key but same deps: HIT + DEDUP
ci-cache save --cache-type cargo --key cargo-xyz --paths ~/.cargo/registry
# → SAVED key=cargo-xyz blobs=1 uploaded=0 B deduped=45.2 MB  ← DEDUP!
```

---

## 📦 Installation

### Build from Source

```bash
cargo build --release
# Binaries land in target/release/:
#   ci-cache-server   (7.2 MB)
#   ci-cache          (6.3 MB)
#   ci-cache-agent    (2.8 MB)
```

### Docker Image

```bash
docker build -t ci-cache .
# Or pull the prebuilt image:
docker pull registry.quantum-forge.net/cluster-ci-cache/ci-cache:latest
```

### Run Tests

```bash
cargo test
# test result: ok. 13 passed; 0 failed; 0 ignored
```

---

## 🗂 Cache Types

### Cargo / Rust

```bash
LOCK_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)
ci-cache restore --cache-type cargo --key "cargo-linux-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
cargo build --release
ci-cache save --cache-type cargo --key "cargo-linux-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
```

### npm

```bash
LOCK_HASH=$(sha256sum package-lock.json | cut -d' ' -f1)
ci-cache restore --cache-type npm --key "npm-linux-${LOCK_HASH}" --paths "$HOME/.npm"
npm ci
ci-cache save --cache-type npm --key "npm-linux-${LOCK_HASH}" --paths "$HOME/.npm"
```

### pnpm

```bash
LOCK_HASH=$(sha256sum pnpm-lock.yaml | cut -d' ' -f1)
STORE=$(pnpm store path)
ci-cache restore --cache-type pnpm --key "pnpm-linux-${LOCK_HASH}" --paths "$STORE"
pnpm install --frozen-lockfile
ci-cache save --cache-type pnpm --key "pnpm-linux-${LOCK_HASH}" --paths "$STORE"
```

### yarn

```bash
LOCK_HASH=$(sha256sum yarn.lock | cut -d' ' -f1)
ci-cache restore --cache-type yarn --key "yarn-linux-${LOCK_HASH}" --paths "$(yarn cache dir)"
yarn install --frozen-lockfile
ci-cache save --cache-type yarn --key "yarn-linux-${LOCK_HASH}" --paths "$(yarn cache dir)"
```

### Docker / BuildKit

```bash
# Use BuildKit's native registry cache (recommended)
docker buildx build \
  --cache-from type=registry,ref=$REGISTRY/cache/myapp:buildcache \
  --cache-to type=registry,ref=$REGISTRY/cache/myapp:buildcache,mode=max \
  -t $REGISTRY/myapp:$TAG --push .

# Or use the CLI helper to generate flags:
CACHE_FROM=$(ci-cache docker cache-from --key myapp:buildcache)
CACHE_TO=$(ci-cache docker cache-to --key myapp:buildcache)
```

---

## 🖥 CLI Reference

```bash
ci-cache [OPTIONS] <COMMAND>

Options:
  --server <URL>         Server endpoint [env: CI_CACHE_SERVER] [default: http://localhost:8080]
  --namespace <NS>       Cache namespace [env: CI_CACHE_NAMESPACE] [default: default]
  --token <TOKEN>        Bearer auth token [env: CI_CACHE_TOKEN]
  --compression <ALGO>   Compression: zstd|gzip|none [env: CI_CACHE_COMPRESSION] [default: zstd]

Commands:
  restore    Restore a cache entry
  save       Save a cache entry
  docker     Docker/BuildKit cache helpers
  health     Check server health
```

### restore

```bash
ci-cache restore --cache-type <type> --key <key> --paths <comma,separated,paths>
```

Downloads the manifest, fetches each referenced blob, verifies its sha256
digest, decompresses with size limits, and extracts to the target paths.

Output: `RESTORED key=... files=N bytes=N` or `MISS key=...`

### save

```bash
ci-cache save --cache-type <type> --key <key> --paths <paths> [--ttl <seconds>]
```

Archives each path with zstd, computes digests, uploads missing blobs (skipping
duplicates), and atomically publishes the manifest.

Output: `SAVED key=... blobs=N uploaded=N deduped=N`

### docker

```bash
ci-cache docker login-cache          # Print BuildKit cache setup guide
ci-cache docker cache-from --key K   # Print --cache-from flag
ci-cache docker cache-to --key K     # Print --cache-to flag
```

---

## 🔌 Server API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/cache/restore` | Lookup manifest by namespace/key |
| `POST` | `/v1/cache/save/start` | Begin a save session |
| `PUT` | `/v1/cache/save/blob` | Upload a blob (digest-verified) |
| `POST` | `/v1/cache/save/finish` | Atomically publish manifest |
| `GET` | `/v1/cache/manifest/:ns/:key` | Get raw manifest |
| `DELETE` | `/v1/cache/manifest/:ns/:key` | Delete manifest |
| `GET` | `/v1/cache/blob/:digest` | Download a blob |
| `DELETE` | `/v1/cache/blob/:digest` | Delete a blob |
| `GET` | `/v1/cache/blob/:digest/exists` | Check blob existence |
| `POST` | `/v1/cache/has-blobs` | Batch dedup check |
| `GET` | `/healthz` | Liveness probe |
| `GET` | `/readyz` | Readiness probe |
| `GET` | `/metrics` | Prometheus metrics |

### Example: Restore Request

```bash
curl -X POST http://ci-cache-server:8080/v1/cache/restore \
  -H 'Content-Type: application/json' \
  -d '{"namespace":"my-team","cache_type":"cargo","key":"cargo-linux-abc123"}'
```

```json
{
  "hit": true,
  "manifest": {
    "namespace": "my-team",
    "cache_type": "cargo",
    "key": "cargo-linux-abc123",
    "blobs": [{ "digest": "sha256:...", "size_bytes": 45000000, "compression": "zstd" }],
    "ttl_seconds": 604800
  }
}
```

---

## ⚙️ Configuration

Configuration is loaded from a YAML file (path set via `CI_CACHE_CONFIG` env)
or falls back to sensible defaults.

```yaml
server:
  listen_addr: "0.0.0.0:8080"
  max_body_bytes: 536870912   # 512 MiB

backend:
  kind: "s3"                   # "local" or "s3"
  fs_root: "/var/lib/ci-cache"
  bucket: "cluster-ci-cache"
  endpoint: "http://minio:9000"
  region: "us-east-1"
  access_key_env: "CI_CACHE_S3_ACCESS_KEY"
  secret_key_env: "CI_CACHE_S3_SECRET_KEY"
  path_style: true             # MinIO compatibility

cache:
  default_ttl_seconds: 604800  # 7 days
  compression: "zstd"          # zstd | gzip | none
  max_blob_size_mb: 512
  max_archive_size_gb: 10
  dedupe: true

auth:
  mode: "serviceaccount"       # none | serviceaccount | token
  token_env: "CI_CACHE_AUTH_TOKEN"
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CI_CACHE_SERVER` | `http://localhost:8080` | Server URL (CLI) |
| `CI_CACHE_NAMESPACE` | `default` | Cache namespace (CLI) |
| `CI_CACHE_TOKEN` | — | Bearer auth token (CLI) |
| `CI_CACHE_CONFIG` | — | Path to config YAML (server) |
| `CI_CACHE_S3_ACCESS_KEY` | — | S3 access key |
| `CI_CACHE_S3_SECRET_KEY` | — | S3 secret key |
| `RUST_LOG` | `info` | Log level (`debug`, `trace`, etc.) |

---

## ☸️ Kubernetes Deployment

### Kustomize (simplest)

```bash
kubectl apply -k deploy/k8s/
```

Creates:
- `ci-cache-server` Deployment (2 replicas)
- `ci-cache-server` Service (ClusterIP)
- `ci-cache-agent` DaemonSet (one per node)
- `ci-cache-agent` Service (headless)
- ServiceAccounts + RBAC (Role, ClusterRole, bindings)
- ConfigMap with backend config
- Secret for S3 credentials
- PersistentVolumeClaim
- NetworkPolicy

### Helm (production)

```bash
helm install cluster-ci-cache deploy/helm/cluster-ci-cache \
  --set backend.kind=s3 \
  --set backend.bucket=my-cache-bucket \
  --set backend.endpoint=https://s3.amazonaws.com \
  --set server.replicas=3 \
  --set persistence.size=500Gi
```

### Production S3/MinIO Setup

```bash
# Create credentials secret
kubectl create secret generic ci-cache-s3-creds \
  --from-literal=access-key=AKIAIOSFODNN7EXAMPLE \
  --from-literal=secret-key=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

# Update ConfigMap: backend.kind: "s3"
kubectl apply -f deploy/k8s/
```

### Verify Deployment

```bash
# All pods running
kubectl get pods -n ci-cache

# Health check
kubectl port-forward -n ci-cache svc/ci-cache-server 8080:8080
curl http://localhost:8080/healthz    # → ok
curl http://localhost:8080/readyz     # → ready
curl http://localhost:8080/metrics    # → Prometheus format
```

---

## 📈 Observability

### Prometheus Metrics

All metrics exposed at `GET /metrics`:

| Metric | Type | Description |
|--------|------|-------------|
| `ci_cache_restore_requests_total` | counter | Total restore requests |
| `ci_cache_save_requests_total` | counter | Total save requests |
| `ci_cache_hits_total` | counter | Cache hits |
| `ci_cache_misses_total` | counter | Cache misses |
| `ci_cache_bytes_uploaded_total` | counter | Bytes uploaded to backend |
| `ci_cache_bytes_downloaded_total` | counter | Bytes downloaded from backend |
| `ci_cache_blob_dedup_hits_total` | counter | Blobs skipped (dedup hit) |
| `ci_cache_backend_errors_total` | counter | Backend errors |
| `ci_cache_restore_duration_seconds` | histogram | Restore latency |
| `ci_cache_save_duration_seconds` | histogram | Save latency |

### Structured JSON Logs

```json
{
  "timestamp": "2026-06-30T18:37:10.486885Z",
  "level": "INFO",
  "fields": {
    "message": "listening on 0.0.0.0:8080",
    "addr": "0.0.0.0:8080"
  },
  "target": "ci_cache_server"
}
```

Control verbosity with `RUST_LOG=ci_cache_server=debug,info`.

### Prometheus Scrape Config

```yaml
- job_name: ci-cache
  kubernetes_sd_configs:
    - role: pod
      namespaces:
        names: [ci-cache]
  relabel_configs:
    - source_labels: [__meta_kubernetes_pod_label_app_kubernetes_io_component]
      action: keep
      regex: server
```

---

## 🔒 Security

### Threat Model & Mitigations

| Threat | Mitigation |
|--------|------------|
| **Path traversal** in archives | Custom `CIC1` format, every path validated against base dir |
| **Absolute path extraction** | `/etc/passwd`, `C:\...` rejected before join |
| **Symlink escape** | Symlinks skipped in archive creation; parent dirs canonicalized |
| **Decompression bombs** | Size-capped `CapacityLimit` writer aborts on overflow |
| **Oversized blobs** | `max_blob_size_mb` (512 MB default) enforced server-side |
| **Digest mismatch / tampering** | Server verifies sha256 on upload; CLI verifies on restore |
| **Unauthorized namespace access** | Namespace validation, isolation in storage paths |
| **Cache poisoning** | Namespaces isolated; manifests stored per-namespace |
| **Incomplete uploads** | Save sessions prevent partial manifests from being published |
| **Corrupt manifests** | Atomic publish via temp-file + rename |

### Auth Modes

| Mode | Description |
|------|-------------|
| `none` | No auth — development only |
| `serviceaccount` | Trusts Kubernetes service account identity |
| `token` | Static bearer token from env var |

See [docs/security.md](docs/security.md) for full details.

---

## 🔬 How Deduplication Works

```
Step 1: CI Job A saves cache for Cargo.lock hash "aaa"
  ~/.cargo/registry  →  zstd archive → sha256:111aaa → uploaded (45 MB)

Step 2: CI Job B saves cache for Cargo.lock hash "bbb" (similar deps)
  ~/.cargo/registry  →  zstd archive → sha256:111aaa → ALREADY EXISTS!
  ~/.cargo/git       →  zstd archive → sha256:222bbb → uploaded (2 MB)

  Result: 0 bytes re-uploaded for the shared blob, 45 MB deduped

Step 3: CI Job C in a DIFFERENT namespace saves the same deps
  team-frontend/cargo-xyz → same sha256:111aaa → ALREADY EXISTS!
  Dedup works ACROSS namespaces and cache types
```

The dedup ratio depends on how similar your dependencies are. Typical ratios:

| Scenario | Dedup Ratio |
|----------|-------------|
| Same project, different commits | 90-99% |
| Similar projects (shared deps) | 60-80% |
| Different projects | 10-30% |

---

## 📊 Comparison

| Feature | Cluster CI Cache | GitHub `actions/cache` | `sccache` | BuildKit cache |
|---------|:-:|:-:|:-:|:-:|
| In-cluster (no external network) | ✅ | ❌ | ❌ | Depends |
| Rust/Cargo cache | ✅ | Manual | ✅ (compiler) | ❌ |
| npm/pnpm/yarn cache | ✅ | ✅ | ❌ | ❌ |
| Docker/BuildKit cache | ✅ (via registry) | ❌ | ❌ | ✅ |
| Cross-namespace dedup | ✅ | ❌ | Partial | ❌ |
| Content-addressed blobs | ✅ | ❌ (KV) | ✅ | ✅ |
| Path-traversal hardening | ✅ (custom format) | N/A | N/A | N/A |
| Decompression bomb protection | ✅ | ❌ | N/A | N/A |
| Kubernetes-native manifests | ✅ | ❌ | ❌ | ❌ |
| Prometheus metrics | ✅ | ❌ | Limited | ❌ |
| Backend-agnostic (FS/S3/MinIO) | ✅ | ❌ | ❌ | Registry |
| TTL expiration | ✅ | ✅ | ❌ | Manual |

---

## 🔧 Development

### Project Structure

```
cluster-ci-cache/
├── crates/
│   ├── ci-cache-core/          # Models, traits, archive, compression, digest, paths
│   ├── ci-cache-server/        # HTTP API server (axum)
│   ├── ci-cache-cli/           # CLI: restore/save/docker (clap)
│   ├── ci-cache-agent/         # Node-local DaemonSet agent
│   ├── ci-cache-backend-fs/    # Filesystem/PVC backend
│   └── ci-cache-backend-s3/    # S3/MinIO backend (SigV4)
├── deploy/
│   ├── helm/                   # Helm chart
│   └── k8s/                    # Kustomize manifests
├── examples/                   # GitHub Actions, Tekton, GitLab, BuildKit
├── docs/                       # Architecture, security, operations, cache-types
├── Dockerfile                  # Multi-stage build
└── Cargo.toml                  # Workspace root
```

### Stats

| Metric | Value |
|--------|-------|
| Rust source | 3,424 lines across 6 crates |
| Test suite | 13 tests (11 unit + 2 integration) |
| Server binary | 7.2 MB (release, stripped) |
| CLI binary | 6.3 MB (release, stripped) |
| Agent binary | 2.8 MB (release, stripped) |

### Running Tests

```bash
cargo test                                    # All tests
cargo test -p ci-cache-core                   # Core unit tests
cargo test -p ci-cache-backend-fs             # Integration tests
cargo check                                   # Type check only
cargo build --release                         # Production build
```

---

## 🗺 Roadmap

### Phase 1 — MVP ✅ Done
- [x] Core models (manifest, blob, cache type)
- [x] Content-addressed blob storage
- [x] zstd compression with bomb protection
- [x] Filesystem/PVC backend
- [x] S3/MinIO backend (SigV4 signed)
- [x] CLI restore/save/docker commands
- [x] HTTP server with full API
- [x] Digest verification (upload + restore)
- [x] Kubernetes manifests + Helm chart
- [x] Prometheus metrics
- [x] CI examples (GitHub Actions, Tekton, GitLab, BuildKit)

### Phase 2 — Near Term
- [ ] Node-local blob cache in agent (PVC-backed)
- [ ] Locality-aware cache restore
- [ ] Cache eviction (LRU)
- [ ] Advanced retention policies
- [ ] OCI registry backend
- [ ] Token auth enforcement middleware

### Phase 3 — Future
- [ ] Cache prefetching / warming
- [ ] `sccache` compatibility mode
- [ ] Web UI dashboard
- [ ] Multi-region replication
- [ ] gRPC API option
- [ ] BuildKit integration in agent

---

## 📄 License

Dual-licensed under **MIT** or **Apache-2.0** at your option.

---

<div align="center">

**[⬆ Back to Top](#-cluster-ci-cache)**

Built with ❤️ in Rust. Designed for Kubernetes. Made for CI.

</div>
