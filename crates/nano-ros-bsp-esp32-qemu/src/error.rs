//! Error types for nano-ros-bsp-esp32-qemu

use core::fmt;

/// Error type for BSP operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// OpenETH initialization failed
    OpenEthInit,
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
    /// Socket limit reached
    SocketLimit,
    /// Invalid configuration
    InvalidConfig,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OpenEthInit => write!(f, "OpenETH initialization failed"),
            Error::NetworkInterface => write!(f, "Network interface error"),
            Error::Route => write!(f, "Failed to add route"),
            Error::ZenohInit => write!(f, "zenoh-pico initialization failed"),
            Error::ZenohOpen => write!(f, "zenoh session open failed"),
            Error::ZenohNotOpen => write!(f, "zenoh session not open"),
            Error::PublisherDeclare => write!(f, "Publisher declaration failed"),
            Error::SubscriberDeclare => write!(f, "Subscriber declaration failed"),
            Error::Publish => write!(f, "Publish operation failed"),
            Error::SocketLimit => write!(f, "Socket limit reached"),
            Error::InvalidConfig => write!(f, "Invalid configuration"),
        }
    }
}

/// Result type for BSP operations
pub type Result<T> = core::result::Result<T, Error>;
