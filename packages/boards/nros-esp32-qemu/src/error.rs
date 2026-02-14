//! Error types for nros-esp32-qemu

use core::fmt;

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// OpenETH initialization failed
    OpenEthInit,
    /// Network interface error
    NetworkInterface,
    /// Failed to add route
    Route,
    /// Transport layer error (zenoh session, publisher, subscriber)
    Transport(nros_rmw::TransportError),
    /// Socket limit reached
    SocketLimit,
    /// Invalid configuration
    InvalidConfig,
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OpenEthInit => write!(f, "OpenETH initialization failed"),
            Error::NetworkInterface => write!(f, "Network interface error"),
            Error::Route => write!(f, "Failed to add route"),
            Error::Transport(e) => write!(f, "Transport error: {:?}", e),
            Error::SocketLimit => write!(f, "Socket limit reached"),
            Error::InvalidConfig => write!(f, "Invalid configuration"),
            Error::TopicTooLong => write!(f, "Topic keyexpr too long for internal buffer"),
            Error::BufferTooSmall => write!(f, "CDR serialization buffer too small"),
            Error::Serialize => write!(f, "CDR serialization failed"),
            Error::Deserialize => write!(f, "CDR deserialization failed"),
        }
    }
}

/// Result type for platform operations
pub type Result<T> = core::result::Result<T, Error>;
