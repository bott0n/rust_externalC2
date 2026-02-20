//! Transport abstraction layer.
//!
//! Re-exports the transport backend selected via Cargo features.
//! Currently supported:
//!   - `transport_s3`   (default) – S3-based transport
//!   - `transport_blob` – Azure Blob-based transport (future)

#[cfg(feature = "transport_s3")]
mod transport_s3;

#[cfg(feature = "transport_s3")]
pub use transport_s3::*;

// TODO
#[cfg(feature = "transport_blob")]
mod transport_blob;

#[cfg(feature = "transport_blob")]
pub use transport_blob::*;
