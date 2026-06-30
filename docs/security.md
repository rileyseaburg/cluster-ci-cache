# Security

## Threat Model

Cluster CI Cache stores build artifacts and cache data that CI jobs produce.
The primary security concerns are:

1. **Path traversal** in archive extraction
2. **Absolute path extraction attacks**
3. **Symlink-based escapes**
4. **Oversized blobs** (disk/memory exhaustion)
5. **Decompression bombs**
6. **Unauthorized namespace access**
7. **Cache poisoning** between namespaces
8. **Incomplete/corrupt uploads**
9. **Digest mismatch** (tampering)

## Mitigations

### Path Traversal Protection

- All archive entry paths are validated against a base directory
- Absolute paths (`/etc/passwd`) are rejected
- Windows drive-qualified paths (`C:\...`) are rejected
- `..` components are detected and rejected
- After joining, the normalized path is checked to be within the base
- Parent directories are canonicalized and verified to be within base

Implementation: `ci_cache_core::paths::ensure_within()` and
`ci_cache_core::archive_ops::sanitize_entry_path()`.

### Symlink Handling

- Symlinks are **skipped** during archive creation (never stored)
- During extraction, symlinks in the archive would not be created
- Parent directories are canonicalized to detect symlink-based escapes

### Size Limits

- `max_blob_size_mb` (default 512 MB) — individual blob upload cap
- `max_archive_size_gb` (default 10 GB) — total archive cap
- Decompression is capped at 4x max archive size
- Server enforces `max_body_bytes` on HTTP request bodies

### Decompression Bomb Prevention

- zstd decompression uses a `CapacityLimit` writer that aborts on overflow
- gzip decompression uses `.take(max_size)` with overflow detection
- The archive header declares `total_size`; this is checked before extraction

### Digest Verification

- Every blob upload is verified: the server computes sha256 and compares
- Every restore is verified: the CLI computes sha256 before extraction
- Content addressing means the digest IS the key — mismatch = rejection

### Namespace Isolation

- Namespaces are validated (alphanumeric + dash/underscore/dot/slash)
- Manifests are stored per-namespace (separate directory/key prefixes)
- Cache keys are validated (no `..`, length limits)
- One namespace cannot read or modify another's manifests

### Authentication

- `service-account` mode: trusts Kubernetes service account identity
- `token` mode: static bearer token from environment variable
- `none` mode: no auth (development only — not for production)

### Atomic Operations

- Manifest writes use temp-file + atomic rename
- Save sessions prevent partial manifests from being published
- Blob uploads that fail mid-stream leave no partial manifest
