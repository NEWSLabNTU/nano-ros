//! Message metadata for received messages
//!
//! This module provides the `MessageInfo` type which contains metadata
//! about received messages, matching the rclrs pattern.
//!
//! # Example
//!
//! ```text
//! node.create_subscription("/topic", |msg: &Int32, info: &MessageInfo| {
//!     println!("Received at {:?} from {:?}", info.source_timestamp(), info.publisher_gid());
//! });
//! ```

use crate::Time;

/// Size of the publisher Global Identifier (GID)
pub const PUBLISHER_GID_SIZE: usize = 16;

/// Metadata about a received message
///
/// Contains information about the source and timing of a message.
/// This matches the rclrs `MessageInfo` type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessageInfo {
    /// Timestamp when the message was published (from the publisher's clock)
    source_timestamp: Time,
    /// Timestamp when the message was received (from the subscriber's clock)
    received_timestamp: Time,
    /// Sequence number of the message from the publisher
    publication_sequence_number: i64,
    /// Sequence number of the message at the subscriber
    reception_sequence_number: i64,
    /// Global identifier of the publisher
    publisher_gid: [u8; PUBLISHER_GID_SIZE],
}

impl MessageInfo {
    /// Create a new MessageInfo with all fields set to defaults
    pub const fn new() -> Self {
        Self {
            source_timestamp: Time::new(0, 0),
            received_timestamp: Time::new(0, 0),
            publication_sequence_number: 0,
            reception_sequence_number: 0,
            publisher_gid: [0u8; PUBLISHER_GID_SIZE],
        }
    }

    /// Create a MessageInfo with the given timestamps
    pub const fn with_timestamps(source: Time, received: Time) -> Self {
        Self {
            source_timestamp: source,
            received_timestamp: received,
            publication_sequence_number: 0,
            reception_sequence_number: 0,
            publisher_gid: [0u8; PUBLISHER_GID_SIZE],
        }
    }

    /// Get the timestamp when the message was published
    pub const fn source_timestamp(&self) -> Time {
        self.source_timestamp
    }

    /// Get the timestamp when the message was received
    pub const fn received_timestamp(&self) -> Time {
        self.received_timestamp
    }

    /// Get the publication sequence number
    pub const fn publication_sequence_number(&self) -> i64 {
        self.publication_sequence_number
    }

    /// Get the reception sequence number
    pub const fn reception_sequence_number(&self) -> i64 {
        self.reception_sequence_number
    }

    /// Get the publisher's Global Identifier (GID)
    pub const fn publisher_gid(&self) -> &[u8; PUBLISHER_GID_SIZE] {
        &self.publisher_gid
    }

    /// Set the source timestamp
    pub fn set_source_timestamp(&mut self, ts: Time) {
        self.source_timestamp = ts;
    }

    /// Set the received timestamp
    pub fn set_received_timestamp(&mut self, ts: Time) {
        self.received_timestamp = ts;
    }

    /// Set the publication sequence number
    pub fn set_publication_sequence_number(&mut self, seq: i64) {
        self.publication_sequence_number = seq;
    }

    /// Set the reception sequence number
    pub fn set_reception_sequence_number(&mut self, seq: i64) {
        self.reception_sequence_number = seq;
    }

    /// Set the publisher GID
    pub fn set_publisher_gid(&mut self, gid: [u8; PUBLISHER_GID_SIZE]) {
        self.publisher_gid = gid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_info_default() {
        let info = MessageInfo::new();
        assert_eq!(info.source_timestamp(), Time::new(0, 0));
        assert_eq!(info.publication_sequence_number(), 0);
        assert_eq!(info.publisher_gid(), &[0u8; PUBLISHER_GID_SIZE]);
    }

    #[test]
    fn test_message_info_with_timestamps() {
        let source = Time::new(1, 500_000_000);
        let received = Time::new(1, 600_000_000);
        let info = MessageInfo::with_timestamps(source, received);
        assert_eq!(info.source_timestamp(), source);
        assert_eq!(info.received_timestamp(), received);
    }
}
