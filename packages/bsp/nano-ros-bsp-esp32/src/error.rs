//! Error types for nano-ros-bsp-esp32

use core::fmt;

/// Error type for BSP operations
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
    /// zenoh-pico initialization failed
    ZenohInit,
    /// zenoh session open failed
    ZenohOpen,
    /// zenoh session not open
    ZenohNotOpen,
    /// Publisher declaration failed
    PublisherDeclare,
    /// Subscriber declaration failed
    SubscriberDeclare,
    /// Publish operation failed
    Publish,
    /// Topic keyexpr exceeds 256-byte buffer
    TopicTooLong,
    /// CDR serialization buffer too small
    BufferTooSmall,
    /// CDR serialization failed
    Serialize,
    /// Socket limit reached
    SocketLimit,
    /// Invalid configuration
    InvalidConfig,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WifiInit => write!(f, "WiFi initialization failed"),
            Error::WifiConnect => write!(f, "WiFi connection failed"),
            Error::DhcpTimeout => write!(f, "DHCP address acquisition timed out"),
            Error::NetworkInterface => write!(f, "Network interface error"),
            Error::Route => write!(f, "Failed to add route"),
            Error::ZenohInit => write!(f, "zenoh-pico initialization failed"),
            Error::ZenohOpen => write!(f, "zenoh session open failed"),
            Error::ZenohNotOpen => write!(f, "zenoh session not open"),
            Error::PublisherDeclare => write!(f, "Publisher declaration failed"),
            Error::SubscriberDeclare => write!(f, "Subscriber declaration failed"),
            Error::Publish => write!(f, "Publish operation failed"),
            Error::TopicTooLong => write!(f, "Topic keyexpr exceeds 256-byte buffer"),
            Error::BufferTooSmall => write!(f, "CDR serialization buffer too small"),
            Error::Serialize => write!(f, "CDR serialization failed"),
            Error::SocketLimit => write!(f, "Socket limit reached"),
            Error::InvalidConfig => write!(f, "Invalid configuration"),
        }
    }
}

/// Result type for BSP operations
pub type Result<T> = core::result::Result<T, Error>;
