//! AWS SigV4 signing helpers for S3-compatible APIs.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Derive the SigV4 signing key from the secret key, date, and region.
pub fn derive_signing_key(secret_key: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret_key}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, b"s3");
    hmac(&k_service, b"aws4_request")
}

/// Build the canonical query string from parsed query pairs.
pub fn canonical_query_string<'a>(
    pairs: impl Iterator<Item = (std::borrow::Cow<'a, str>, std::borrow::Cow<'a, str>)>,
) -> String {
    let mut items: Vec<(String, String)> = pairs
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    items.sort();
    items
        .into_iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                percent_encoding::utf8_percent_encode(&k, percent_encoding::NON_ALPHANUMERIC),
                percent_encoding::utf8_percent_encode(&v, percent_encoding::NON_ALPHANUMERIC),
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Compute the hex-encoded sha256 of a byte slice.
pub fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
