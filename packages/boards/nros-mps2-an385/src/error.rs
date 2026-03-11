//! Error types for the QEMU platform crate

use core::fmt;

/// Error type for platform initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "ethernet"), allow(dead_code))]
pub(crate) enum Error {
    /// Ethernet driver initialization failed
    #[cfg(feature = "ethernet")]
    EthernetInit,
    /// Failed to add route
    #[cfg(feature = "ethernet")]
    Route,
    /// Placeholder to keep the enum inhabited when no transport errors apply.
    /// This variant is never constructed.
    #[doc(hidden)]
    _Never,
}

impl fmt::Display for Error {
    #[cfg_attr(not(feature = "ethernet"), allow(unused_variables))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "ethernet")]
            Error::EthernetInit => write!(f, "Ethernet driver initialization failed"),
            #[cfg(feature = "ethernet")]
            Error::Route => write!(f, "Failed to add route"),
            Error::_Never => unreachable!(),
        }
    }
}

/// Result type for platform operations
#[cfg_attr(not(feature = "ethernet"), allow(dead_code))]
pub(crate) type Result<T> = core::result::Result<T, Error>;
