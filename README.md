# Rust External C2

A Rust-based External C2 client and Python-based server for Cobalt Strike, using third-party / white-listed endpoint as the transport layer. Allowed to hide the c2 connection traffic behind trust domain.
Support Passthrough mode (Cobalt Strike 4.10 Feature)S
## Architecture

```
┌──────────────┐                 ┌─────────┐          ┌──────────────────┐
│  Rust Client │◄──Third-Party──►│  Server │◄──TCP──► │  Cobalt Strike   │
│  (Windows)   │                 │ (Python)│          │  External C2     │
└──────────────┘                 └─────────┘          │  (port 2222)     │
                                                      └──────────────────┘
```

### Client Internal Architecture

```
main.rs
  │
  ├── utils.rs          ← register_beacon / send_data / recv_data
  │     │                  (handles encoding/decoding)
  │     │
  │     └── transports/  ← raw I/O only (no encoding)
  │           ├── transport_s3.rs
  │           └── transport_blob.rs
  │
  ├── beacon.rs          ← named pipe communication
  └── config.rs          ← compile-time settings
```

Encoding/decoding is centralized in `utils.rs`. Transports only handle raw byte I/O, making it easy to swap encoders or add new transports independently.

## Project Structure

```
rust_externalC2/
├── client/                          # Rust client agent
│   ├── Cargo.toml                   # Build config & feature flags
│   └── src/
│       ├── main.rs                  # Entry point (stager/stagless logic)
│       ├── config.rs                # ⚙️ Pipe name, payload filename, sleep time
│       ├── beacon.rs                # Beacon pipe communication (non-blocking)
│       ├── utils.rs                 # Encoding/decoding + transport wrappers
│       └── transports/
│           ├── mod.rs               # Transport abstraction layer
│           ├── transport_s3.rs      # S3 transport (raw I/O)
│           └── transport_blob.rs    # Azure Blob transport (raw I/O)
├── server/                          # Python C2 server
│   ├── config.py                    # ⚙️ Server configuration
│   ├── external_c2_server.py        # Server entry point
│   └── utils/transports/
│       ├── transport_s3.py          # S3 transport
│       └── transport_blob.py        # Azure Blob transport
├── README.md                        # This file
└── TRANSPORT_WORKBOOK.md            # Guide for building new transports
```

## Modes

The client supports two modes: **Stager** and **Stagless (Passthrough)**.

| Feature | Stager | Stagless (Passthrough) |
|---|---|---|
| Payload delivery | Downloaded from C2 via transport | Read from local file |
| Cobalt Strike listener | External C2 only | External C2 + SMB |
| Config needed | `Cargo.toml` + `config.py` | `Cargo.toml` + `config.py` + `config.rs` |
| BOF | ✅  | ✅  |
| UDRL | ❌  | ✅  | 

## Supported Transports

| Transport | Client Feature | Server Module | Status |
|---|---|---|---|
| AWS S3 | `transport_s3` | `transport_s3` | ✅ Ready |
| Azure Blob | `transport_blob` | `transport_blob` | ✅ Ready |

---

## Setup — Stager Mode

### 1. Configure the Client

Edit `client/Cargo.toml` — set the default feature to `stager` and your transport:

```toml
[features]
default = ["stager", "transport_s3"]
```

### 2. Configure the Server

Edit `server/config.py`:

```python
Mode = "stager"
TRANSPORT_MODULE = "transport_s3"   # or "transport_blob"
```

### 3. Cobalt Strike

1. Create a new **External C2** listener on port **2222**.

### 4. Build & Run

```bash
cd client
cargo build --release
```

---

## Setup — Stagless (Passthrough) Mode

### 1. Configure the Client

Edit `client/Cargo.toml` — set the default feature to `stagless` and your transport:

```toml
[features]
default = ["stagless", "transport_s3"]
```

### 2. Configure the Server

Edit `server/config.py`:

```python
Mode = "stagless"
TRANSPORT_MODULE = "transport_s3"   # or "transport_blob"
```

### 3. Cobalt Strike

1. Create a new **External C2** listener on port **2222**.
2. Create a new **SMB** listener with the **same pipe name** as configured in `client/src/config.rs`.

### 4. Generate Payload

1. Generate **Stagless** payload with SMB beacon.
2. Rename the filename to match **PAYLOAD_FILE** in `client/src/config.rs`.

### 5. Customize Pipe Name & Payload Filename (Optional)

Edit `client/src/config.rs`:

```rust
/// Named pipe path – must match the SMB listener pipe name in Cobalt Strike.
pub const PIPE_NAME: &str = "\\\\.\\pipe\\crashpad_70692_GBIQVCTLGLFTBXRE";

/// Filename to read the stagless payload from.
pub const PAYLOAD_FILE: &str = "tmp.dat";
```

> **Note:** The `PIPE_NAME` must match the pipe name configured in the Cobalt Strike SMB listener. The `PAYLOAD_FILE` is the local file the client reads the beacon payload from.

### 6. Build & Run

```bash
cd client
cargo build --release
```

---

## Server Configuration Reference

`server/config.py`:

| Variable | Description | Default |
|---|---|---|
| `EXTERNAL_C2_ADDR` | Address of the External C2 server | `127.0.0.1` |
| `EXTERNAL_C2_PORT` | Port of the External C2 server | `2222` |
| `C2_PIPE_NAME` | Pipe name the beacon should use | `crashpad_70692_GBIQVCTLGLFTBXRE` |
| `C2_BLOCK_TIME` | Block time (ms) when no tasks available | `100` |
| `C2_ARCH` | Beacon architecture | `x64` |
| `Mode` | `"stager"` or `"stagless"` | `stager` |
| `IDLE_TIME` | Polling interval (seconds) | `5` |
| `TRANSPORT_MODULE` | Transport module to use | `transport_s3` |

## Client Feature Flags

`client/Cargo.toml`:

| Feature | Description |
|---|---|
| `stager` | Stager mode — payload downloaded from C2 |
| `stagless` | Stagless mode — payload read from local file |
| `transport_s3` | Use AWS S3 as transport layer |
| `transport_blob` | Use Azure Blob Storage as transport layer |

## Build

```bash
# Build with default features (as configured in Cargo.toml)
cd client
cargo build --release

# Or override features from command line:
cargo build --release --no-default-features --features "stager,transport_s3"
cargo build --release --no-default-features --features "stager,transport_blob"
cargo build --release --no-default-features --features "stagless,transport_s3"
cargo build --release --no-default-features --features "stagless,transport_blob"
```

## Server
```bash
cd server/
python3 external_c2_server.py

# Enable debug and verbose logging
python3 external_c2_server.py -d -v
```

## Setup — Transports
### AWS S3
1. Create a bucket on AWS S3.
2. Create an IAM user whose only access is to S3 buckets, and generate secret/access key pair for them.
3. In `client/src/transports/transport_s3.rs`, change the configurations:
```rust
const AWS_SECRET_KEY: &str = "YOUR-SECRET-KEY";
const AWS_ACCESS_KEY: &str = "YOUR-ACCESS-KEY";
const AWS_BUCKET_NAME: &str = "S3-BUCKET-NAME";
const AWS_REGION: &str = "S3-BUCKET-REGION";
```
4. In `server/utils/transports/transport_s3.py`, change the configurations:
```python
AWS_SECRET_KEY = 'YOUR-SECRET-KEY'
AWS_ACCESS_KEY = 'YOUR-ACCESS-KEY'
AWS_BUCKET_NAME = 'S3-BUCKET-NAME'
```

### Azure Blob
1. Create a blob storage account on Azure.
2. Create a blob container on the storage account. 
3. In `client/src/transports/transport_blob.rs`, change the confgurations:
```rust
/// Azure Storage account name
const AZURE_ACCOUNT_NAME: &str = "YOUR_ACCOUNT_NAME";

/// Azure Blob container name
const AZURE_CONTAINER_NAME: &str = "YOUR_CONTAINER_NAME";

/// SAS token (without leading '?')
/// Must have: Read, Add, Create, Write, Delete, List permissions on the container.
const AZURE_SAS_TOKEN: &str = "YOUR_SAS_TOKEN";
```
4. In `server/utils/transports/transports_blob.py`, change the configurations:
```python
AZURE_CONNECTION_STRING = "YOUR-AZURE-CONNECTION-STRING"
AZURE_CONTAINER_NAME = "YOUR-AZURE-CONTAINER-NAME"
```

# Build Your own Transport

See [TRANSPORT_DEVELOPMENT_WORKFLOW.md](TRANSPORT_DEVELOPMENT_WORKFLOW.md) for a step-by-step guide on creating new transport modules.

Or you can just let AI to read this and generate for you :)

# Todos:
- ~~Azure blob Transport~~ (Done)
- Microsoft Teams Transport
- Support RC4 encryption
- Support AES encryption
- Better method to handle sleep

# Credits and Acknowledgments
This project is based on the research of [external-c2_framework](https://github.com/RhinoSecurityLabs/external_c2_framework) by RhinoSecurityLabs. The passthrough mode implementation is referenced from [cobalt-strike-external-c2-passthrough](https://www.covertswarm.com/post/cobalt-strike-external-c2-passthrough) by CovertSwarm.

# References
- Server source code modified to support python3 and Passthrough mode https://github.com/RhinoSecurityLabs/external_c2_framework/tree/master
- Cobalt Strike External C2 Passthrough Guide https://www.covertswarm.com/post/cobalt-strike-external-c2-passthrough
- Cobalt Strike Beacon C2 using Amazon APIs https://rhinosecuritylabs.com/aws/hiding-cloudcobalt-strike-beacon-c2-using-amazon-apis/
- https://hstechdocs.helpsystems.com/manuals/cobaltstrike/current/userguide/content/topics/blog_user-def-reflcive-loader-part3.htm
