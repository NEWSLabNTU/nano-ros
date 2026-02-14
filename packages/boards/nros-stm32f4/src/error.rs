//! Error types for the STM32F4 platform crate

/// Error type for platform operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum Error {
    /// Failed to initialize hardware
    HardwareInit,
    /// Failed to initialize network stack
    NetworkInit,
    /// Failed to add route
    Route,
    /// Failed to connect to zenoh router
    ZenohInit,
    /// zenoh session open failed
    ZenohOpen,
    /// zenoh session not open
    ZenohNotOpen,
    /// Failed to create publisher
    PublisherDeclare,
    /// Failed to create subscriber
    SubscriberDeclare,
    /// Failed to publish message
    Publish,
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
}

/// Result type for platform operations
pub type Result<T> = core::result::Result<T, Error>;
