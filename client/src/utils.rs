//! Utility helpers: encoding/decoding and file I/O.

use base64::{engine::general_purpose, Engine as _};
#[cfg(feature = "stagless")]
use tokio::fs;

/// Encode raw bytes into a reversible, URL-safe string.
///
/// Pipeline: raw bytes → base64 → URL-encode → reverse string.
pub fn data_encode(data: &[u8]) -> String {
    let b64 = general_purpose::STANDARD.encode(data);
    let quoted = urlencoding::encode(&b64).into_owned();
    quoted.chars().rev().collect()
}

/// Decode a string produced by [`data_encode`] back into raw bytes.
///
/// Pipeline: reverse string → URL-decode → base64-decode.
pub fn data_decode(data: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let reversed: String = data.chars().rev().collect();
    let unquoted = urlencoding::decode(&reversed)?;
    let bytes = general_purpose::STANDARD.decode(unquoted.as_bytes())?;
    Ok(bytes)
}

/// Read the entire contents of a file as raw bytes (used by stagless mode).
#[cfg(feature = "stagless")]
pub async fn read_file(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let data = fs::read(path).await
        .map_err(|e| format!("Failed to read {}: {}", path, e))?;
    Ok(data)
}
