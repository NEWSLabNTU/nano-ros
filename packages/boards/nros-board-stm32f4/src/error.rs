//! Error types for the STM32F4 platform crate

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "ethernet"), allow(dead_code))]
pub(crate) enum Error {
    /// Failed to initialize hardware
    #[cfg(feature = "ethernet")]
    HardwareInit,
    /// Failed to add route
    #[cfg(feature = "ethernet")]
    Route,
    /// Placeholder to keep the enum inhabited when no transport errors apply.
    #[doc(hidden)]
    _Never,
}

impl defmt::Format for Error {
    #[cfg_attr(not(feature = "ethernet"), allow(unused_variables))]
    fn format(&self, f: defmt::Formatter) {
        match self {
            #[cfg(feature = "ethernet")]
            Error::HardwareInit => defmt::write!(f, "Hardware init failed"),
            #[cfg(feature = "ethernet")]
            Error::Route => defmt::write!(f, "Failed to add route"),
            Error::_Never => unreachable!(),
        }
    }
}

/// Result type for platform operations
#[cfg_attr(not(feature = "ethernet"), allow(dead_code))]
pub(crate) type Result<T> = core::result::Result<T, Error>;
