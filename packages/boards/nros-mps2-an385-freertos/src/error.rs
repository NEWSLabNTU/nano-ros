//! Error types for the FreeRTOS QEMU platform crate

use core::fmt;

/// Error type for platform initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // TaskCreate used once FreeRTOS examples exercise error paths
pub(crate) enum Error {
    /// lwIP / LAN9118 network initialization failed
    NetworkInit,
    /// FreeRTOS task creation failed
    TaskCreate,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NetworkInit => write!(f, "Network initialization failed"),
            Error::TaskCreate => write!(f, "FreeRTOS task creation failed"),
        }
    }
}

/// Result type for platform operations.
pub(crate) type Result<T> = core::result::Result<T, Error>;
