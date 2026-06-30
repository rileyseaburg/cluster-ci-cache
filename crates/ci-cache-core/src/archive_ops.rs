//! Archive extraction and directory walking helpers.

use std::path::{Component, Path, PathBuf};

use crate::archive::ArchiveHeader;
use crate::compression::Compression;
use crate::error::{CacheError, Result};
use crate::paths;

/// Extract an archive blob into a target directory.
/// All paths are validated to stay within `dest`; decompression is size-capped.
pub fn extract_archive(
    compressed: &[u8],
    compression: Compression,
    dest: &Path,
    max_decompress_bytes: u64,
) -> Result<ArchiveHeader> {
    let raw = compression.decompress(compressed, max_decompress_bytes)?;
    let (header, content_offset) = ArchiveHeader::from_bytes(&raw)?;

    if header.total_size > max_decompress_bytes {
        return Err(CacheError::DecompressionBomb {
            size: header.total_size,
            limit: max_decompress_bytes,
        });
    }

    std::fs::create_dir_all(dest)?;
    let dest_canon = dest
        .canonicalize()
        .unwrap_or_else(|_| dest.to_path_buf());

    let mut cursor = content_offset;
    for entry in &header.entries {
        let size = entry.size as usize;
        if cursor + size > raw.len() {
            return Err(CacheError::InvalidInput(format!(
                "archive truncated at entry {}",
                entry.path
            )));
        }
        let file_data = &raw[cursor..cursor + size];
        cursor += size;

        let target = paths::ensure_within(&dest_canon, &entry.path)?;
        let safe = sanitize_entry_path(&target, &dest_canon)?;

        if let Some(parent) = safe.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&safe, file_data)?;

        #[cfg(unix)]
        if let Some(mode) = entry.mode {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &safe,
                std::fs::Permissions::from_mode(mode),
            );
        }
    }

    Ok(header)
}

fn sanitize_entry_path(target: &Path, base: &Path) -> Result<PathBuf> {
    if let Some(parent) = target.parent() {
        let canon = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        if !canon.starts_with(base) {
            return Err(CacheError::PathTraversal(format!(
                "entry target escapes base: {}",
                target.display()
            )));
        }
    }
    for comp in target.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(CacheError::PathTraversal(format!(
                "parent dir in entry path: {}",
                target.display()
            )));
        }
    }
    Ok(target.to_path_buf())
}

/// Recursively walk a directory, yielding file paths (not symlinks/dirs).
pub(crate) fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let read = match std::fs::read_dir(&d) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(CacheError::Io(e)),
        };
        for entry in read {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
            } else {
                result.push(path);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create_archive_from_dir;
    use std::io::Write;

    #[test]
    fn test_roundtrip_zstd() {
        let tmp = tempdir();
        let src = tmp.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("sub/b.txt"), b"world").unwrap();

        let (blob, header) =
            create_archive_from_dir(&src, Compression::Zstd, 1024 * 1024).unwrap();
        assert_eq!(header.entries.len(), 2);
        assert_eq!(header.total_size, 10);

        let out = tmp.join("out");
        let h = extract_archive(&blob, Compression::Zstd, &out, 1024 * 1024).unwrap();
        assert_eq!(h.entries.len(), 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read(out.join("sub/b.txt")).unwrap(), b"world");
    }

    #[test]
    fn test_bomb_rejected() {
        let tmp = tempdir();
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("big.txt"), vec![b'A'; 1000]).unwrap();
        let (blob, _) =
            create_archive_from_dir(&src, Compression::Zstd, 1024 * 1024).unwrap();
        let out = tmp.join("out");
        let err = extract_archive(&blob, Compression::Zstd, &out, 100).unwrap_err();
        assert!(matches!(err, CacheError::DecompressionBomb { .. }));
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cic-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
