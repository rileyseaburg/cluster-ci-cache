# Docker / BuildKit Cache Integration
#
# Cluster CI Cache recommends using BuildKit's native registry-type cache
# for Docker layer caching. The ci-cache server focuses on filesystem-based
# build caches (Cargo, npm, pnpm, etc.).
#
# For Docker layer caching, point BuildKit at an OCI-compatible registry
# (e.g. the same registry backing your cluster) using these flags.

# --- Method 1: BuildKit registry cache (recommended) ---

# Create a buildx builder with container driver for cache support:
docker buildx create --use --name ci-cache-builder \
  --driver docker-container

# Build with cache-from + cache-to:
docker buildx build \
  --cache-from type=registry,ref=$REGISTRY/cache/myapp:buildcache \
  --cache-to type=registry,ref=$REGISTRY/cache/myapp:buildcache,mode=max \
  -t $REGISTRY/myapp:$TAG \
  --push .

# --- Method 2: Using the ci-cache CLI docker helper ---

# Print cache-from/cache-to flags for a given key:
CACHE_FROM=$(ci-cache docker cache-from --key myapp-buildcache)
CACHE_TO=$(ci-cache docker cache-to --key myapp-buildcache)

docker buildx build \
  --cache-from "$CACHE_FROM" \
  --cache-to "$CACHE_TO" \
  -t $REGISTRY/myapp:$TAG \
  --push .

# --- pnpm example ---

# For pnpm, cache the store directory:
ci-cache restore \
  --cache-type pnpm \
  --key "pnpm-linux-amd64-$(sha256sum pnpm-lock.yaml | cut -d' ' -f1)" \
  --paths "$PNPM_STORE_PATH"

pnpm install --frozen-lockfile

ci-cache save \
  --cache-type pnpm \
  --key "pnpm-linux-amd64-$(sha256sum pnpm-lock.yaml | cut -d' ' -f1)" \
  --paths "$PNPM_STORE_PATH"

# --- yarn example ---

ci-cache restore \
  --cache-type yarn \
  --key "yarn-linux-amd64-$(sha256sum yarn.lock | cut -d' ' -f1)" \
  --paths "$(yarn cache dir)"

yarn install --frozen-lockfile

ci-cache save \
  --cache-type yarn \
  --key "yarn-linux-amd64-$(sha256sum yarn.lock | cut -d' ' -f1)" \
  --paths "$(yarn cache dir)"
