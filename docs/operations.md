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

### Local filesystem backend and replicas

The local filesystem backend stores manifests and blobs under the server pod's
local `/var/lib/ci-cache` path. Do not run multiple server replicas with this
backend unless that path is backed by shared `ReadWriteMany` storage. A
ClusterIP service can route a restore request to a different pod than the save
request, which makes warm caches look like random misses.

For a single-pod local filesystem deployment, pin the server to one replica:

```bash
kubectl -n ci-cache scale deploy/ci-cache-server --replicas=1
kubectl -n ci-cache rollout status deploy/ci-cache-server
```

Use S3, MinIO, or shared PVC storage before scaling the server horizontally.

### Runner namespace access

CI runner pods must be allowed to reach the server service. If a NetworkPolicy
selects the cache server pods, make sure it allows ingress from the runner
namespace and runner pod labels.

Example Forgejo runner access rule:

```yaml
ingress:
  - from:
      - namespaceSelector:
          matchLabels:
            kubernetes.io/metadata.name: forgejo-actions
        podSelector:
          matchLabels:
            app: forgejo-act-runner-dind
    ports:
      - port: 8080
        protocol: TCP
```

Validate connectivity from a runner pod before relying on cache acceleration:

```bash
kubectl -n forgejo-actions exec deploy/forgejo-act-runner-dind -c runner -- \
  wget -qO- http://ci-cache-server.ci-cache.svc.cluster.local:8080/healthz
```

The expected response is `ok`. Workflows should fail fast on `ci-cache health`
instead of silently falling back to slow uncached builds.

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
