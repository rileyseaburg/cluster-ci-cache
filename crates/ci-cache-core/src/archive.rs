//! Archive format: JSON header (CIC1 magic + len + entries) then file bytes.
//! Compressed as one blob. Path-traversal hardened, size-capped.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::compression::Compression;
use crate::error::{CacheError, Result};
use crate::archive_ops::walk_files;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    #[serde(default)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveHeader {
    pub entries: Vec<ArchiveEntry>,
    pub total_size: u64,
}

impl ArchiveHeader {
    pub const MAGIC: &'static [u8] = b"CIC1";

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(self)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        let mut out = Vec::with_capacity(Self::MAGIC.len() + 4 + json.len());
        out.extend_from_slice(Self::MAGIC);
        out.extend_from_slice(&(json.len() as u32).to_be_bytes());
        out.extend_from_slice(&json);
        Ok(out)
    }

    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize)> {
        if data.len() < Self::MAGIC.len() + 4 {
            return Err(CacheError::InvalidInput("archive too short".into()));
        }
        if &data[..Self::MAGIC.len()] != Self::MAGIC {
            return Err(CacheError::InvalidInput("invalid archive magic".into()));
        }
        let len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let start = Self::MAGIC.len() + 4;
        if data.len() < start + len {
            return Err(CacheError::InvalidInput("truncated header".into()));
        }
        let header: ArchiveHeader = serde_json::from_slice(&data[start..start + len])
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        Ok((header, start + len))
    }
}

pub fn create_archive_from_dir(
    dir: &Path, compression: Compression, max_bytes: u64,
) -> Result<(Vec<u8>, ArchiveHeader)> {
    let mut entries: Vec<(PathBuf, u64, Option<u32>)> = Vec::new();
    let mut total_size: u64 = 0;
    for entry in walk_files(dir)? {
        let rel = entry.strip_prefix(dir).unwrap_or(&entry).to_path_buf();
        let meta = std::fs::symlink_metadata(&entry)?;
        if meta.is_dir() || meta.file_type().is_symlink() { continue; }
        let size = meta.len();
        total_size = total_size.checked_add(size)
            .ok_or_else(|| CacheError::ArchiveTooLarge { limit: max_bytes })?;
        if total_size > max_bytes {
            return Err(CacheError::ArchiveTooLarge { limit: max_bytes });
        }
        #[cfg(unix)]
        let mode = { use std::os::unix::fs::PermissionsExt; Some(meta.permissions().mode()) };
        #[cfg(not(unix))]
        let mode = None;
        entries.push((rel, size, mode));
    }
    let header = ArchiveHeader {
        entries: entries.iter().map(|(p, s, m)| ArchiveEntry {
            path: p.to_string_lossy().replace('\\', "/"), size: *s, mode: *m,
        }).collect(),
        total_size,
    };
    let mut raw = header.to_bytes()?;
    for (path, _, _) in &entries {
        raw.extend_from_slice(&std::fs::read(dir.join(path))?);
    }
    Ok((compression.compress(&raw)?, header))
}
