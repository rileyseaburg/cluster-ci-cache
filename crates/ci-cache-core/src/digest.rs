//! Digest computation and verification utilities.

use sha2::{Digest, Sha256};

use crate::error::{CacheError, Result};

/// A streaming hasher that accumulates a sha256 digest over a sequence of
/// byte chunks, suitable for hashing large blobs without loading them fully
/// into memory.
pub struct DigestVerifier {
    hasher: Sha256,
    expected: String,
    bytes_seen: u64,
}

impl DigestVerifier {
    /// Create a verifier that will check the final digest against `expected`
    /// (a `sha256:<hex>` string).
    pub fn new(expected: impl Into<String>) -> Self {
        Self {
            hasher: Sha256::new(),
            expected: expected.into(),
            bytes_seen: 0,
        }
    }

    /// Feed a chunk of bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
        self.bytes_seen += bytes.len() as u64;
    }

    /// Number of bytes hashed so far.
    pub fn bytes_seen(&self) -> u64 {
        self.bytes_seen
    }

    /// Finalize and verify. Returns an error if the computed digest does not
    /// match the expected value.
    pub fn verify(self) -> Result<String> {
        let computed = format!("sha256:{}", hex::encode(self.hasher.finalize()));
        if computed != self.expected {
            return Err(CacheError::DigestMismatch {
                expected: self.expected,
                actual: computed,
            });
        }
        Ok(computed)
    }
}

/// Compute a `sha256:<hex>` digest for a byte slice.
pub fn compute_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Verify that a byte slice matches an expected digest.
pub fn verify_digest(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = compute_digest(bytes);
    if actual != expected {
        return Err(CacheError::DigestMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// Extract the hex portion from a `sha256:<hex>` string.
pub fn digest_hex(digest: &str) -> &str {
    digest.strip_prefix("sha256:").unwrap_or(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_and_verify() {
        let data = b"hello world";
        let digest = compute_digest(data);
        assert!(digest.starts_with("sha256:"));
        assert!(verify_digest(data, &digest).is_ok());
        assert!(verify_digest(b"tampered", &digest).is_err());
    }

    #[test]
    fn test_streaming_verifier() {
        let data = b"hello world";
        let expected = compute_digest(data);
        let mut v = DigestVerifier::new(expected.clone());
        v.update(b"hello ");
        v.update(b"world");
        assert_eq!(v.verify().unwrap(), expected);
    }

    #[test]
    fn test_digest_hex() {
        assert_eq!(digest_hex("sha256:abcd"), "abcd");
        assert_eq!(digest_hex("abcd"), "abcd");
    }
}
