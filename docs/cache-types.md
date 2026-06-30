# Cache Types

## Rust / Cargo

Cache the Cargo registry and git sources, plus optionally `target/`.

**Paths:**
- `~/.cargo/registry` — downloaded crate registry
- `~/.cargo/git` — git dependencies
- `target/` — build output (optional, large)

**Key strategy:** hash of `Cargo.lock` ensures cache is invalidated when
dependencies change.

```bash
LOCK_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)
ci-cache restore --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
cargo build --release
ci-cache save --cache-type cargo \
  --key "cargo-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.cargo/registry,$HOME/.cargo/git,target"
```

> **Note:** `target/` caching can speed up incremental builds significantly but
> produces large archives. Consider separate keys per build profile.

## npm

Cache the npm global cache directory.

**Paths:**
- `~/.npm` — npm cache

```bash
LOCK_HASH=$(sha256sum package-lock.json | cut -d' ' -f1)
ci-cache restore --cache-type npm \
  --key "npm-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.npm"
npm ci
ci-cache save --cache-type npm \
  --key "npm-linux-amd64-${LOCK_HASH}" \
  --paths "$HOME/.npm"
```

> `node_modules` is **not** cached by default — it should be reconstructed
> from the package cache for reproducibility.

## pnpm

Cache the pnpm content-addressable store.

**Paths:**
- `$PNPM_STORE_PATH` (typically `~/.local/share/pnpm/store`)

```bash
LOCK_HASH=$(sha256sum pnpm-lock.yaml | cut -d' ' -f1)
STORE=$(pnpm store path)
ci-cache restore --cache-type pnpm \
  --key "pnpm-linux-amd64-${LOCK_HASH}" \
  --paths "$STORE"
pnpm install --frozen-lockfile
ci-cache save --cache-type pnpm \
  --key "pnpm-linux-amd64-${LOCK_HASH}" \
  --paths "$STORE"
```

## yarn

Cache the yarn cache directory.

**Paths:**
- `$(yarn cache dir)`

```bash
LOCK_HASH=$(sha256sum yarn.lock | cut -d' ' -f1)
ci-cache restore --cache-type yarn \
  --key "yarn-linux-amd64-${LOCK_HASH}" \
  --paths "$(yarn cache dir)"
yarn install --frozen-lockfile
ci-cache save --cache-type yarn \
  --key "yarn-linux-amd64-${LOCK_HASH}" \
  --paths "$(yarn cache dir)"
```

## Docker / BuildKit

Use BuildKit's native registry-type cache for Docker layer caching:

```bash
docker buildx build \
  --cache-from type=registry,ref=$REGISTRY/cache/myapp:buildcache \
  --cache-to type=registry,ref=$REGISTRY/cache/myapp:buildcache,mode=max \
  -t $REGISTRY/myapp:$TAG \
  --push .
```

The `ci-cache docker` subcommands generate the appropriate flags:

```bash
ci-cache docker login-cache     # print setup instructions
ci-cache docker cache-from --key myapp:buildcache  # print --cache-from
ci-cache docker cache-to --key myapp:buildcache    # print --cache-to
```
