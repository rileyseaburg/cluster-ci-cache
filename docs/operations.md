# Operations

## Deployment

### Quick Start (PVC backend)

```bash
# Deploy to Kubernetes
kubectl apply -f deploy/k8s/config.yaml
kubectl apply -f deploy/k8s/rbac.yaml
kubectl apply -f deploy/k8s/server.yaml
kubectl apply -f deploy/k8s/agent.yaml

# Or with kustomize
kubectl apply -k deploy/k8s/
```

### Production (S3/MinIO backend)

1. Create the S3 bucket (or MinIO bucket)
2. Create a Kubernetes secret with credentials:
   ```bash
   kubectl create secret generic ci-cache-s3-creds \
     --from-literal=access-key=YOUR_ACCESS_KEY \
     --from-literal=secret-key=YOUR_SECRET_KEY
   ```
3. Update the ConfigMap to use `kind: s3`
4. Deploy

## Configuration

Configuration is loaded from a YAML file (path in `CI_CACHE_CONFIG` env) or
defaults. Environment variables override file settings.

See `deploy/k8s/config.yaml` for the full reference.

## Observability

### Health Checks

- `GET /healthz` — liveness probe
- `GET /readyz` — readiness probe

### Metrics

Prometheus metrics at `GET /metrics`:

| Metric | Description |
|--------|-------------|
| `ci_cache_restore_requests_total` | Restore request count |
| `ci_cache_save_requests_total` | Save request count |
| `ci_cache_hits_total` | Cache hits |
| `ci_cache_misses_total` | Cache misses |
| `ci_cache_bytes_uploaded_total` | Bytes uploaded |
| `ci_cache_bytes_downloaded_total` | Bytes downloaded |
| `ci_cache_blob_dedup_hits_total` | Blobs skipped (dedup) |
| `ci_cache_backend_errors_total` | Backend errors |
| `ci_cache_restore_duration_seconds` | Restore latency histogram |
| `ci_cache_save_duration_seconds` | Save latency histogram |

### Logs

Structured JSON logs to stdout. Configure level with `RUST_LOG`:
```bash
RUST_LOG=ci_cache_server=debug,info
```

## Scaling

- The server is **stateless** (all state in the backend) — scale horizontally
- For PVC backend, use `ReadWriteMany` storage or shard by namespace
- For S3/MinIO backend, scale freely (shared storage)
- The agent runs as a DaemonSet (one per node)

## Retention

- TTLs are set per-manifest (default 7 days)
- Expired manifests are reported as misses
- Manual cleanup: `DELETE /v1/cache/manifest/{namespace}/{key}`

## Troubleshooting

### Cache always misses

- Verify the cache key matches between save and restore
- Check that paths exist at save time
- Verify namespace is the same
- Check server logs for `expired` messages

### Blob upload fails

- Check `max_blob_size_mb` and `max_body_bytes` limits
- Verify S3/MinIO connectivity and credentials
- Check disk space on PVC backend

### Digest mismatch

- This indicates corruption or tampering
- Check network stability (interrupted uploads)
- The server rejects blobs whose content doesn't match the declared digest
