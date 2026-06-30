//! Path sanitization and traversal protection.
//!
//! CI cache restores must never write outside the intended directory.

use std::path::{Component, Path, PathBuf};

use crate::error::{CacheError, Result};

/// Resolve and validate a user-supplied path, expanding `~` to home.
pub fn resolve_path(path: &str) -> Result<PathBuf> {
    let expanded = expand_home(path);
    let p = PathBuf::from(&expanded);
    let absolute = if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .map_err(|e| CacheError::InvalidPath(e.to_string()))?
            .join(p)
    };
    Ok(normalize(&absolute))
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

/// Normalize a path, collapsing `.` and `..` components lexically.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Ensure that a candidate path stays within the base directory.
pub fn ensure_within(base: &Path, candidate: &str) -> Result<PathBuf> {
    if candidate.starts_with('/') || candidate.starts_with('\\') {
        return Err(CacheError::PathTraversal(format!(
            "absolute path not allowed: {candidate}"
        )));
    }
    if candidate.len() >= 2 && candidate.as_bytes()[1] == b':' {
        return Err(CacheError::PathTraversal(format!(
            "drive-qualified path not allowed: {candidate}"
        )));
    }
    let joined = base.join(candidate);
    let normalized = normalize(&joined);
    if !normalized.starts_with(base) {
        return Err(CacheError::PathTraversal(format!(
            "path escapes base directory: {candidate}"
        )));
    }
    Ok(normalized)
}

/// Validate a namespace identifier.
pub fn validate_namespace(ns: &str) -> Result<()> {
    if ns.is_empty() || ns.len() > 253 {
        return Err(CacheError::InvalidInput(format!(
            "invalid namespace length: {ns:?}"
        )));
    }
    if !ns
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        return Err(CacheError::InvalidInput(format!(
            "namespace contains illegal characters: {ns:?}"
        )));
    }
    Ok(())
}

/// Validate a cache key.
pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 1024 {
        return Err(CacheError::InvalidInput(format!(
            "invalid key length: {key:?}"
        )));
    }
    if key.contains("..") {
        return Err(CacheError::InvalidInput(
            "cache key must not contain '..'".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_within_safe() {
        let base = Path::new("/tmp/cache");
        assert!(ensure_within(base, "foo/bar.txt").is_ok());
        assert!(ensure_within(base, "a/b/c").is_ok());
    }

    #[test]
    fn test_ensure_within_traversal() {
        let base = Path::new("/tmp/cache");
        assert!(ensure_within(base, "../escape").is_err());
        assert!(ensure_within(base, "foo/../../escape").is_err());
        assert!(ensure_within(base, "/etc/passwd").is_err());
        assert!(ensure_within(base, "C:/windows").is_err());
    }

    #[test]
    fn test_validate_namespace() {
        assert!(validate_namespace("spotlessbinco").is_ok());
        assert!(validate_namespace("team-a/infra").is_ok());
        assert!(validate_namespace("").is_err());
        assert!(validate_namespace("bad namespace!").is_err());
    }

    #[test]
    fn test_validate_key() {
        assert!(validate_key("cargo-linux-amd64-abc123").is_ok());
        assert!(validate_key("").is_err());
        assert!(validate_key("a../b").is_err());
    }
}
