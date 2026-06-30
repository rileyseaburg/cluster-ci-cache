# Architecture

## Overview

Cluster CI Cache is an in-cluster, cache-agnostic CI build cache system for
Kubernetes. It provides a unified interface for restoring and saving build
caches across Rust/Cargo, npm/pnpm/yarn, and Docker/BuildKit workloads.

## Components

### ci-cache-server

The central cache service running as a `Deployment` in the cluster. It:
- Exposes an HTTP REST API for cache restore/save operations
- Manages cache metadata (manifests) and content-addressed blobs
- Supports multiple storage backends (filesystem/PVC, S3/MinIO)
- Provides deduplication, TTLs, and retention
- Exposes Prometheus metrics on `/metrics`
- Handles concurrent CI jobs safely via atomic manifest publishing

### ci-cache-cli

The CLI binary used inside CI jobs (`ci-cache restore` / `ci-cache save`).
It communicates with the server over HTTP. The CLI:
- Scans configured paths and creates compressed archives
- Computes sha256 digests for content addressing
- Uploads blobs (skipping already-present ones for dedup)
- Downloads and verifies blobs on restore
- Extracts archives safely with path-traversal protection

### ci-cache-agent

A node-local `DaemonSet` agent for optional fast local cache access. For the
MVP it provides a health/status endpoint and local cache directory management.
Future capabilities:
- Node-local blob cache (PVC-backed)
- Prefetching / cache warming
- Eviction with LRU
- Locality-aware cache restore
- BuildKit integration

## Data Model

Cache entries are **manifest-based**, not simple key-value pairs:

```
sha256(content) → blob
manifest → list of blobs
cache key → manifest
```

A `CacheManifest` contains:
- namespace + cache_type + key (unique identifier)
- list of `CachedPath` entries (original path → archive blob)
- list of `BlobRef` entries (content-addressed blobs)
- metadata (TTL, timestamps, arbitrary CI metadata)

This enables:
- **Deduplication**: identical blobs are stored once across keys/namespaces/types
- **Partial restore**: select specific paths
- **Verification**: every blob is digest-checked on upload and restore

## Storage Backends

| Backend | Use Case | Status |
|---------|----------|--------|
| Filesystem/PVC | Local dev, simple clusters | ✅ MVP |
| S3/MinIO | Production, shared storage | ✅ MVP |
| OCI Registry | Docker/BuildKit cache | 📋 Future |

## Request Flow

### Restore
1. CLI requests manifest by namespace/key/cache_type
2. Server returns manifest (or miss)
3. CLI downloads referenced blobs
4. CLI verifies sha256 digest of each blob
5. CLI extracts archives to requested paths (path-traversal safe)
6. CLI reports hit/miss and bytes restored

### Save
1. CLI scans configured paths
2. CLI creates zstd-compressed archive per path
3. CLI computes sha256 digest
4. CLI checks if blob already exists (dedup check)
5. CLI uploads missing blobs
6. CLI finalizes manifest atomically
7. CLI reports bytes uploaded/deduped and cache key

## Security

See [security.md](./security.md) for the full security model.

## Concurrency

- Manifests are published atomically (temp file + rename)
- Save sessions prevent partial manifests from being visible
- Blob storage is idempotent (content-addressed, dedup-safe)
- No distributed locks required for correctness in v1
