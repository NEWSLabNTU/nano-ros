//! Error types for the QEMU platform crate

use core::fmt;

/// Error type for platform initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    /// Ethernet driver initialization failed
    EthernetInit,
    /// Failed to add route
    Route,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EthernetInit => write!(f, "Ethernet driver initialization failed"),
            Error::Route => write!(f, "Failed to add route"),
        }
    }
}

/// Result type for platform operations
pub(crate) type Result<T> = core::result::Result<T, Error>;
