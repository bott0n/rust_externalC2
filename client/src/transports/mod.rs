//! Transport abstraction layer.
//!
//! Re-exports the transport backend selected via Cargo features.
//! Currently supported:
//!   - `transport_s3`   – S3-based transport
//!   - `transport_blob` – Azure Blob-based transport

#[cfg(feature = "transport_s3")]
mod transport_s3;

#[cfg(feature = "transport_s3")]
pub use transport_s3::*;

/// Type alias so `main.rs` can refer to the transport handle generically.
#[cfg(feature = "transport_s3")]
pub type Transport = transport_s3::S3Transport;

#[cfg(feature = "transport_blob")]
mod transport_blob;

#[cfg(feature = "transport_blob")]
pub use transport_blob::*;

#[cfg(feature = "transport_blob")]
pub type Transport = transport_blob::BlobTransport;
