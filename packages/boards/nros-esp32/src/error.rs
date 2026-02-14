//! Error types for nros-esp32

use core::fmt;

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// WiFi initialization failed
    WifiInit,
    /// WiFi connection failed
    WifiConnect,
    /// DHCP address acquisition failed
    DhcpTimeout,
    /// Network interface error
    NetworkInterface,
    /// Failed to add route
    Route,
    /// Transport layer error (zenoh session, publisher, subscriber)
    Transport(nros_rmw::TransportError),
    /// Topic keyexpr exceeds 256-byte buffer
    TopicTooLong,
    /// CDR serialization buffer too small
    BufferTooSmall,
    /// CDR serialization failed
    Serialize,
    /// CDR deserialization failed
    Deserialize,
    /// Socket limit reached
    SocketLimit,
    /// Invalid configuration
    InvalidConfig,
}

impl From<nros_rmw::TransportError> for Error {
    fn from(e: nros_rmw::TransportError) -> Self {
        Error::Transport(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WifiInit => write!(f, "WiFi initialization failed"),
            Error::WifiConnect => write!(f, "WiFi connection failed"),
            Error::DhcpTimeout => write!(f, "DHCP address acquisition timed out"),
            Error::NetworkInterface => write!(f, "Network interface error"),
            Error::Route => write!(f, "Failed to add route"),
            Error::Transport(e) => write!(f, "Transport error: {:?}", e),
            Error::TopicTooLong => write!(f, "Topic keyexpr exceeds 256-byte buffer"),
            Error::BufferTooSmall => write!(f, "CDR serialization buffer too small"),
            Error::Serialize => write!(f, "CDR serialization failed"),
            Error::Deserialize => write!(f, "CDR deserialization failed"),
            Error::SocketLimit => write!(f, "Socket limit reached"),
            Error::InvalidConfig => write!(f, "Invalid configuration"),
        }
    }
}

/// Result type for platform operations
pub type Result<T> = core::result::Result<T, Error>;
