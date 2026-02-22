//! Error types for the STM32F4 platform crate

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    /// Failed to initialize hardware
    HardwareInit,
    /// Failed to add route
    Route,
}

impl defmt::Format for Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Error::HardwareInit => defmt::write!(f, "Hardware init failed"),
            Error::Route => defmt::write!(f, "Failed to add route"),
        }
    }
}

/// Result type for platform operations
pub(crate) type Result<T> = core::result::Result<T, Error>;
