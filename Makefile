SHELL := /bin/bash
CI_CACHE_BINARY_SERVER ?= http://192.168.50.82/ci-cache
CI_CACHE_BINARY_NAMESPACE ?= cluster-ci-cache-binaries-v1
CI_CACHE_BINARY_STAGE ?= .ci-cache-artifacts/ci-cache-cli
CI_CACHE_BINARY_SOURCE_DIGEST ?= $(shell { find Cargo.toml Cargo.lock crates/ci-cache-core crates/ci-cache-cli -type f -print0 2>/dev/null | sort -z | xargs -0 sha256sum; [ -f Makefile ] && sha256sum Makefile; } | sha256sum | cut -c1-12)
CI_CACHE_BINARY_KEY ?= ci-cache-cli-$(shell uname -s)-$(shell uname -m)-$(CI_CACHE_BINARY_SOURCE_DIGEST)
CI_CACHE_BINARY_TTL ?= 604800

.PHONY: build-cli cache-cli-binary restore-cli-binary clean-cli-cache-stage

build-cli:
	cargo build --release --package ci-cache-cli --bin ci-cache
	target/release/ci-cache --version
	target/release/ci-cache manifest --help >/dev/null
	target/release/ci-cache docker cache-ref --help >/dev/null
	target/release/ci-cache docker cache-ref cache-from --key make-validate >/dev/null

$(CI_CACHE_BINARY_STAGE)/ci-cache: target/release/ci-cache
	install -d $(CI_CACHE_BINARY_STAGE)
	install -m 0755 target/release/ci-cache $(CI_CACHE_BINARY_STAGE)/ci-cache

cache-cli-binary: build-cli $(CI_CACHE_BINARY_STAGE)/ci-cache
	@set -euo pipefail; 	eval "$$(target/release/ci-cache auth env 	  --server "$(CI_CACHE_BINARY_SERVER)" 	  --namespace "$(CI_CACHE_BINARY_NAMESPACE)" 	  --kube-namespace "$${CI_CACHE_KUBE_NAMESPACE:-ci-cache}" 	  --kube-secret "$${CI_CACHE_KUBE_SECRET:-ci-cache-auth}")"; 	echo "Using $$CI_CACHE_SERVER namespace=$$CI_CACHE_NAMESPACE key=$(CI_CACHE_BINARY_KEY)"; 	target/release/ci-cache health >/dev/null; 	target/release/ci-cache save 	  --cache-type generic 	  --key "$(CI_CACHE_BINARY_KEY)" 	  --paths "$(CI_CACHE_BINARY_STAGE)" 	  --ttl "$(CI_CACHE_BINARY_TTL)" 	  --metadata crate=ci-cache-cli 	  --metadata binary=ci-cache 	  --metadata git_sha="$$(git rev-parse HEAD)"

restore-cli-binary:
	@set -euo pipefail; 	eval "$$(ci-cache auth env 	  --server "$(CI_CACHE_BINARY_SERVER)" 	  --namespace "$(CI_CACHE_BINARY_NAMESPACE)" 	  --kube-namespace "$${CI_CACHE_KUBE_NAMESPACE:-ci-cache}" 	  --kube-secret "$${CI_CACHE_KUBE_SECRET:-ci-cache-auth}")"; 	echo "Using $$CI_CACHE_SERVER namespace=$$CI_CACHE_NAMESPACE key=$(CI_CACHE_BINARY_KEY)"; 	ci-cache health >/dev/null; 	ci-cache restore 	  --cache-type generic 	  --key "$(CI_CACHE_BINARY_KEY)" 	  --paths "$(CI_CACHE_BINARY_STAGE)"

clean-cli-cache-stage:
	rm -rf $(CI_CACHE_BINARY_STAGE)