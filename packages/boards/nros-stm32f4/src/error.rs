//! Error types for the STM32F4 platform crate

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Failed to initialize hardware
    HardwareInit,
    /// Failed to initialize network stack
    NetworkInit,
    /// Failed to add route
    Route,
    /// Transport layer error (zenoh session, publisher, subscriber)
    Transport(nros_rmw::TransportError),
    /// Invalid configuration
    InvalidConfig,
    /// Timeout waiting for operation
    Timeout,
    /// Resource exhausted (buffers full, etc.)
    ResourceExhausted,
    /// Topic keyexpr too long for internal buffer
    TopicTooLong,
    /// CDR serialization buffer too small
    BufferTooSmall,
    /// CDR serialization failed
    Serialize,
    /// CDR deserialization failed
    Deserialize,
}

impl From<nros_rmw::TransportError> for Error {
    fn from(e: nros_rmw::TransportError) -> Self {
        Error::Transport(e)
    }
}

impl defmt::Format for Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Error::HardwareInit => defmt::write!(f, "Hardware init failed"),
            Error::NetworkInit => defmt::write!(f, "Network init failed"),
            Error::Route => defmt::write!(f, "Failed to add route"),
            Error::Transport(e) => defmt::write!(f, "Transport error: {:?}", defmt::Debug2Format(e)),
            Error::InvalidConfig => defmt::write!(f, "Invalid configuration"),
            Error::Timeout => defmt::write!(f, "Timeout"),
            Error::ResourceExhausted => defmt::write!(f, "Resource exhausted"),
            Error::TopicTooLong => defmt::write!(f, "Topic keyexpr too long"),
            Error::BufferTooSmall => defmt::write!(f, "Buffer too small"),
            Error::Serialize => defmt::write!(f, "Serialization failed"),
            Error::Deserialize => defmt::write!(f, "Deserialization failed"),
        }
    }
}

/// Result type for platform operations
pub type Result<T> = core::result::Result<T, Error>;
