//! Compression algorithms used for blob storage.

use serde::{Deserialize, Serialize};

use crate::error::{CacheError, Result};

/// Supported compression algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    None,
    Zstd,
    Gzip,
}

impl Compression {
    pub fn as_str(&self) -> &'static str {
        match self {
            Compression::None => "none",
            Compression::Zstd => "zstd",
            Compression::Gzip => "gzip",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Ok(Compression::None),
            "zstd" => Ok(Compression::Zstd),
            "gzip" | "gz" => Ok(Compression::Gzip),
            other => Err(CacheError::Config(format!("unknown compression '{other}'"))),
        }
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Compression::None => Ok(data.to_vec()),
            Compression::Zstd => Ok(zstd::encode_all(data, zstd::DEFAULT_COMPRESSION_LEVEL)?),
            Compression::Gzip => {
                use std::io::Write;
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
        }
    }

    pub fn decompress(&self, data: &[u8], max_size: u64) -> Result<Vec<u8>> {
        match self {
            Compression::None => {
                if data.len() as u64 > max_size {
                    return Err(CacheError::DecompressionBomb {
                        size: data.len() as u64,
                        limit: max_size,
                    });
                }
                Ok(data.to_vec())
            }
            Compression::Zstd => {
                let mut decoder = zstd::Decoder::new(data)?;
                let mut out = Vec::with_capacity(data.len().min(8 * 1024 * 1024));
                let mut limiter = CapacityLimit::new(&mut out, max_size);
                if let Err(e) = std::io::copy(&mut decoder, &mut limiter) {
                    if limiter.exceeded {
                        return Err(CacheError::DecompressionBomb {
                            size: max_size + 1,
                            limit: max_size,
                        });
                    }
                    return Err(CacheError::Io(e));
                }
                Ok(out)
            }
            Compression::Gzip => {
                use std::io::Read;
                let decoder = flate2::read::MultiGzDecoder::new(data);
                let mut out = Vec::with_capacity(data.len().min(8 * 1024 * 1024));
                let mut limited = decoder.take(max_size);
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = limited.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    out.extend_from_slice(&buf[..n]);
                }
                let mut probe = [0u8; 1];
                if limited.read(&mut probe)? > 0 {
                    return Err(CacheError::DecompressionBomb {
                        size: max_size,
                        limit: max_size,
                    });
                }
                Ok(out)
            }
        }
    }
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Zstd
    }
}

struct CapacityLimit<W> {
    inner: W,
    written: u64,
    limit: u64,
    exceeded: bool,
}

impl<W> CapacityLimit<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self { inner, written: 0, limit, exceeded: false }
    }
}

impl<W: std::io::Write> std::io::Write for CapacityLimit<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.written + buf.len() as u64 > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other(format!(
                "decompression bomb: would exceed {} bytes",
                self.limit
            )));
        }
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
