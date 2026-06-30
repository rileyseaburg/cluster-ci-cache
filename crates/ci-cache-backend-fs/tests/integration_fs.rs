//! Integration test: full save/restore cycle with filesystem backend.
//!
//! This test exercises the core archive, compression, digest, and FS backend
//! pipeline end-to-end without a network server.

use std::path::PathBuf;

use ci_cache_backend_fs::FsBackend;
use ci_cache_core::archive::create_archive_from_dir;
use ci_cache_core::archive_ops::extract_archive;
use ci_cache_core::backend::{collect_stream, CacheBackend};
use ci_cache_core::compression::Compression;
use ci_cache_core::digest::compute_digest;
use ci_cache_core::manifest::{BlobRef, CacheManifest, CacheType, CachedPath};
use futures::stream;

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cic-int-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn test_full_save_restore_cycle() {
    let root = tempdir();
    let backend = FsBackend::new(root.join("store")).unwrap();

    // --- Simulate a CI job that produces files to cache ---
    let src = root.join("src");
    std::fs::create_dir_all(src.join("registry/cache")).unwrap();
    std::fs::write(src.join("registry/cache/index.txt"), b"crate index data").unwrap();
    std::fs::write(src.join("registry/cache/.cargo-index"), b"binary index").unwrap();
    std::fs::write(src.join("Cargo.toml"), b"[package]\nname = \"test\"\n").unwrap();

    let compression = Compression::Zstd;
    let max_archive = 1024 * 1024 * 1024;

    // Archive the src directory
    let (blob_data, header) =
        create_archive_from_dir(&src, compression, max_archive).unwrap();
    let digest = compute_digest(&blob_data);

    // Upload blob to backend (simulating blob upload)
    let data = blob_data.clone();
    let stream = stream::once(async move {
        Ok::<_, ci_cache_core::CacheError>(bytes::Bytes::from(data))
    });
    let byte_stream: ci_cache_core::backend::ByteStream = Box::pin(stream);
    let loc = backend.put_blob(&digest, byte_stream).await.unwrap();
    assert_eq!(loc.size_bytes, blob_data.len() as u64);

    // Build and store the manifest
    let mut manifest = CacheManifest::new("test-ns", CacheType::Cargo, "cargo-linux-test-abc123");
    manifest.ttl_seconds = Some(604800);
    manifest.paths.push(CachedPath {
        original_path: src.join("registry").to_string_lossy().to_string(),
        archive_digest: digest.clone(),
        mode: None,
    });
    manifest.blobs.push(BlobRef {
        digest: digest.clone(),
        size_bytes: blob_data.len() as u64,
        uncompressed_size_bytes: header.total_size,
        compression: compression.as_str().to_string(),
        backend: "local".to_string(),
        location: loc.location.clone(),
    });

    backend.put_manifest(manifest.clone()).await.unwrap();

    // --- Simulate a restore ---
    let fetched = backend
        .get_manifest("test-ns", "cargo-linux-test-abc123")
        .await
        .unwrap()
        .expect("manifest should exist");

    assert_eq!(fetched.cache_type, CacheType::Cargo);
    assert_eq!(fetched.key, "cargo-linux-test-abc123");
    assert_eq!(fetched.paths.len(), 1);

    let cached_path = &fetched.paths[0];
    let blob_ref = fetched.find_blob(&cached_path.archive_digest).unwrap();

    let blob_bytes = backend.get_blob_bytes(&blob_ref.digest).await.unwrap();

    // Verify digest
    let actual = compute_digest(&blob_bytes);
    assert_eq!(actual, cached_path.archive_digest);

    // Extract to restore location
    let restore_dir = root.join("restored");
    let h = extract_archive(
        &blob_bytes,
        Compression::parse(&blob_ref.compression).unwrap(),
        &restore_dir,
        max_archive,
    )
    .unwrap();

    assert_eq!(h.entries.len(), 3); // 3 files
    assert_eq!(
        std::fs::read(restore_dir.join("Cargo.toml")).unwrap(),
        b"[package]\nname = \"test\"\n"
    );

    // --- Test dedup: upload the same blob again ---
    let data2 = blob_data.clone();
    let stream2 = stream::once(async move {
        Ok::<_, ci_cache_core::CacheError>(bytes::Bytes::from(data2))
    });
    let bs2: ci_cache_core::backend::ByteStream = Box::pin(stream2);
    let loc2 = backend.put_blob(&digest, bs2).await.unwrap();
    // Should find existing blob (dedup)
    assert!(backend.has_blob(&digest).await.unwrap());

    // --- Test delete ---
    backend.delete_manifest("test-ns", "cargo-linux-test-abc123").await.unwrap();
    assert!(backend
        .get_manifest("test-ns", "cargo-linux-test-abc123")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_namespace_isolation() {
    let root = tempdir();
    let backend = FsBackend::new(root.join("store")).unwrap();

    // Store manifest in namespace-a
    let m1 = CacheManifest::new("namespace-a", CacheType::Generic, "key-1");
    backend.put_manifest(m1).await.unwrap();

    // Store manifest in namespace-b with same key
    let m2 = CacheManifest::new("namespace-b", CacheType::Generic, "key-1");
    backend.put_manifest(m2).await.unwrap();

    // Each namespace should only see its own manifest
    let a = backend.get_manifest("namespace-a", "key-1").await.unwrap().unwrap();
    let b = backend.get_manifest("namespace-b", "key-1").await.unwrap().unwrap();
    assert_eq!(a.namespace, "namespace-a");
    assert_eq!(b.namespace, "namespace-b");

    // Deleting in namespace-a should not affect namespace-b
    backend.delete_manifest("namespace-a", "key-1").await.unwrap();
    assert!(backend.get_manifest("namespace-a", "key-1").await.unwrap().is_none());
    assert!(backend.get_manifest("namespace-b", "key-1").await.unwrap().is_some());
}
