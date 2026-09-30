//! The error type every port and use case speaks.

use thiserror::Error;

use super::storage::Refusal;

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
    /// The screen or the host cannot do this (a screen without storage or
    /// device-side playback, a media converter that is not installed).
    #[error("not supported: {0}")]
    Unsupported(String),
    /// The user cancelled a long operation.
    #[error("cancelled{}", partial_note(.partial))]
    Cancelled {
        /// Bytes of an incomplete file the job left on the screen; `None`
        /// when it left none (or none could be found).
        partial: Option<u64>,
    },
    /// A destructive or persistent operation came without `Confirm::Yes`;
    /// nothing was sent. The text names the operation.
    #[error("{0} needs confirmation")]
    NotConfirmed(String),
    /// A storage operation failed its preflight; nothing was converted, sent
    /// or deleted.
    #[error("refused: {0}")]
    Refused(Refusal),
    /// A theme file or folder that cannot be read, written or imported
    /// (missing, malformed, an unsafe asset path). The text names the file
    /// and the problem.
    #[error("theme file: {0}")]
    ThemeFile(String),
}

fn partial_note(partial: &Option<u64>) -> String {
    partial
        .map(|n| format!("; an incomplete file of {n} bytes remains on the screen"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_errors_read_well() {
        assert_eq!(
            BezelError::Cancelled { partial: None }.to_string(),
            "cancelled"
        );
        assert_eq!(
            BezelError::Cancelled {
                partial: Some(2490)
            }
            .to_string(),
            "cancelled; an incomplete file of 2490 bytes remains on the screen"
        );
        assert_eq!(
            BezelError::Unsupported("Turing 3.5\" has no storage".into()).to_string(),
            "not supported: Turing 3.5\" has no storage"
        );
    }

    #[test]
    fn theme_file_errors_read_well() {
        assert_eq!(
            BezelError::ThemeFile("theme.json: missing field `name`".into()).to_string(),
            "theme file: theme.json: missing field `name`"
        );
    }
}
