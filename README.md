# Rust External C2

A Rust-based External C2 client and Python-based server for Cobalt Strike, using third-party / white-listed endpoint as the transport layer.

## Architecture

```
┌──────────────┐                 ┌─────────┐          ┌──────────────────┐
│  Rust Client │◄──Third-Party──►│  Server │◄──TCP──► │  Cobalt Strike   │
│  (Windows)   │                 │ (Python)│          │  External C2     │
└──────────────┘                 └─────────┘          │  (port 2222)     │
                                                      └──────────────────┘
```

## Project Structure

```
rust_externalC2/
├── client/                     # Rust client agent
│   ├── cargo.toml              # Build config & feature flags
│   └── src/
│       ├── main.rs             # Entry point (stager/stagless logic)
│       ├── config.rs           # ⚙️ Pipe name & payload filename config
│       ├── beacon.rs           # Beacon pipe communication (non-blocking approach)
│       ├── utils.rs            # Encoding helpers & file I/O
│       └── transport/
│           ├── mod.rs          # Transport abstraction layer
│           └── transport_s3.rs # S3 transport implementation
├── server/                     # Python C2 server
│   ├── config.py               # ⚙️ Server configuration
│   └── external_c2_server.py   # Server entry point
└── github/
    └── README.md               # This file
```

## Modes

The client supports two modes: **Stager** and **Stagless (Pass-thru)**.

| Feature | Stager | Stagless (Pass-thru) |
|---|---|---|
| Payload delivery | Downloaded from C2 via S3 | Read from local file |
| Cobalt Strike listener | External C2 only | External C2 + SMB |
| Config needed | `cargo.toml` + `config.py` | `cargo.toml` + `config.py` + `config.rs` |

---

## Setup — Stager Mode

### 1. Configure the Client

Edit `client/cargo.toml` — set the default feature to `stager`:

```toml
[features]
default = ["stager", "transport_s3"]
```

### 2. Configure the Server

Edit `server/config.py`:

```python
STAGE = "stager"
```

### 3. Cobalt Strike

1. Create a new **External C2** listener on port **2222**.

### 4. Build & Run

```bash
cd client
cargo build --release
```

---

## Setup — Stagless (Pass-thru) Mode

### 1. Configure the Client

Edit `client/cargo.toml` — set the default feature to `stagless`:

```toml
[features]
default = ["stagless", "transport_s3"]
```

### 2. Configure the Server

Edit `server/config.py`:

```python
STAGE = "stagless"
```

### 3. Cobalt Strike

1. Create a new **External C2** listener on port **2222**.
2. Create a new **SMB** listener with the **same pipe name** as configured in `client/src/config.rs`.

### 4. Generation Payload

1. Generate **Stagless** payload with smb beacon
2. Rename the filename as well as the **PAYLOAD_FILE** under **client/src/config.rs**.

### 5. Customize Pipe Name & Payload Filename (Optional)

Edit `client/src/config.rs`:

```rust
/// Named pipe path – must match the SMB listener pipe name in Cobalt Strike.
pub const PIPE_NAME: &str = "\\\\.\\pipe\\crashpad_70692_GBIQVCTLGLFTBXRE";

/// Filename to read the stagless payload from.
pub const PAYLOAD_FILE: &str = "tmp.dat";
```

> **Note:** The `PIPE_NAME` must match the pipe name configured in the Cobalt Strike SMB listener. The `PAYLOAD_FILE` is the local file the client reads the beacon payload from.

### 5. Build & Run

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

`client/cargo.toml`:

| Feature | Description |
|---|---|
| `stager` | Stager mode — payload downloaded from C2 |
| `stagless` | Stagless mode — payload read from local file |
| `transport_s3` | Use S3 as transport layer |
| `transport_blob` | Use Azure Blob as transport layer (future) |

## Build

```bash
# Build with default features (as configured in cargo.toml)
cd client
cargo build --release

# Or override features from command line:
cargo build --release --no-default-features --features "stager,transport_s3"
cargo build --release --no-default-features --features "stagless,transport_s3"
```

## Server
```bash
cd server/
python3 external_c2_server.py
```

# Todos:
- Support Azure blob transport
- RC4 encryption on payload 

# References
- Server source code modified to support python3 and Passthrough mode https://github.com/RhinoSecurityLabs/external_c2_framework
- Cobalt Strike External C2 Passthrough Guide https://www.covertswarm.com/post/cobalt-strike-external-c2-passthrough
- Cobalt Strike Beacon C2 using Amazon APIs https://rhinosecuritylabs.com/aws/hiding-cloudcobalt-strike-beacon-c2-using-amazon-apis/
- https://hstechdocs.helpsystems.com/manuals/cobaltstrike/current/userguide/content/topics/blog_user-def-reflcive-loader-part3.htm
