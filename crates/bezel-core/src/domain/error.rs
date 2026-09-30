//! The error type every port and use case speaks.

use thiserror::Error;

/// Errors of the Bezel domain. Adapters map their own errors into these
/// variants and never leak transport-specific error types through a port.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BezelError {
    /// No connected screen matches the requested identity.
    #[error("screen not found: {0}")]
    ScreenNotFound(String),
    /// The operating system refused access to a device (permissions, busy port).
    #[error("access denied to {address}: {reason}")]
    AccessDenied {
        /// Opaque device address (port name or USB path).
        address: String,
        /// Human-readable reason reported by the adapter.
        reason: String,
    },
    /// Another program holds the screen's port.
    #[error("{address} is in use by {}", holders.join(", "))]
    InUse {
        /// Opaque device address.
        address: String,
        /// Human-readable descriptions of the holders (program and PID).
        holders: Vec<String>,
    },
    /// A device did not answer in time.
    #[error("timeout talking to {0}")]
    Timeout(String),
    /// Something the caller passed that the device or format cannot take
    /// (a frame of the wrong size, a theme that does not fit).
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Any other transport-level failure.
    #[error("transport error: {0}")]
    Transport(String),
}
