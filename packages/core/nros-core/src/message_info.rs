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

/// Size of the publisher Global Identifier (GID), in bytes.
///
/// **24, which is upstream's `RMW_GID_STORAGE_SIZE`** — measured against Humble,
/// `/opt/ros/humble/include/rmw/rmw/types.h:42` reads `24u` — and the width of
/// the ABI's own `rmw_gid_t::data` (`RMW_GID_STORAGE_SIZE`,
/// `packages/core/nros-rmw-abi/include/nros/rmw_entity.h`). So a gid read out of
/// a [`MessageInfo`] and a gid written by the `get_gid_for_publisher` vtable
/// slot are the same TYPE, comparable without a conversion.
///
/// It was 16 until the phase-467 RMW gap-closure design study's Q1(a). The 16
/// is *zenoh's* wire width, not a property of the identifier — see
/// [`MessageInfo::publisher_gid`] for what that leaves a reader owing.
pub const PUBLISHER_GID_SIZE: usize = 24;

/// Zero-extend a backend's narrower identity into a full-width publisher GID.
///
/// A backend whose identity is shorter than [`PUBLISHER_GID_SIZE`] must pad the
/// TAIL with zeros rather than leave it undefined, or two gids naming the same
/// publisher compare unequal on stack garbage. This is the one spelling of that
/// padding on the Rust side; the Cyclone backend does the same thing in C++
/// (`cyclone_get_gid_for_publisher` in
/// `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp` — a `memset`,
/// then a `memcpy` of the 16-byte DDS writer GUID).
///
/// An `N` wider than [`PUBLISHER_GID_SIZE`] is a compile error, which is the
/// Rust spelling of Cyclone's
/// `static_assert(sizeof(guid.v) <= RMW_GID_STORAGE_SIZE)`.
pub const fn pad_publisher_gid<const N: usize>(narrow: &[u8; N]) -> [u8; PUBLISHER_GID_SIZE] {
    const {
        assert!(
            N <= PUBLISHER_GID_SIZE,
            "a publisher identity wider than PUBLISHER_GID_SIZE cannot be zero-extended into it"
        );
    }
    let mut wide = [0u8; PUBLISHER_GID_SIZE];
    let mut i = 0;
    while i < N {
        wide[i] = narrow[i];
        i += 1;
    }
    wide
}

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

    /// Get the publisher's Global Identifier (GID).
    ///
    /// **The array is 24 bytes. How many of them MEAN anything is a backend
    /// property, and today it is never all 24.**
    ///
    /// The width is upstream's ([`PUBLISHER_GID_SIZE`]), so this value and the
    /// `rmw_gid_t` the `get_gid_for_publisher` vtable slot fills are one type.
    /// What the width does NOT say:
    ///
    /// * **zenoh fills 16 and zero-pads 8.** `RMW_ATTACHMENT_SIZE` carries a
    ///   16-byte gid because that is `rmw_zenoh_cpp`'s wire layout — its reader
    ///   REJECTS any other length — so the trailing 8 bytes here are padding
    ///   written by [`pad_publisher_gid`] and carry no information. Compare
    ///   whole arrays anyway: the padding is deterministic, and truncating to
    ///   16 by hand is how a future backend's extra bytes get silently dropped.
    /// * **Cyclone never fills it.** A pure C/C++ backend writes no
    ///   `MessageInfo` at all — the `message_info()` callback sees `None`
    ///   there, not a gid — and nothing on the Cyclone receive path calls
    ///   [`set_publisher_gid`](Self::set_publisher_gid). Wherever a
    ///   `MessageInfo` does exist unpopulated it keeps its `Default`: all
    ///   zeros, which is the ABI's spelling for "unknown", not an identity.
    /// * **A gid from here is NOT today comparable with one from
    ///   `get_gid_for_publisher`.** No backend produces both: on Cyclone the
    ///   vtable slot returns a real DDS writer GUID and this field is never
    ///   written; on zenoh this field is a per-publisher value and the slot is
    ///   NULL. Making the two answers agree is issue 1495 — it changes a value
    ///   a stock ROS 2 peer reads off our wire, so it is deliberately not part
    ///   of the widening that made them the same type.
    ///
    /// An all-zero gid therefore means "this backend did not say", never "the
    /// publisher's id is zero".
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

    /// Set the publisher GID.
    ///
    /// Takes the full [`PUBLISHER_GID_SIZE`] width. A backend whose identity is
    /// narrower zero-extends it through [`pad_publisher_gid`] rather than
    /// building the array itself — see [`publisher_gid`](Self::publisher_gid)
    /// for what a reader may and may not conclude from the result.
    pub fn set_publisher_gid(&mut self, gid: [u8; PUBLISHER_GID_SIZE]) {
        self.publisher_gid = gid;
    }
}

/// Raw-subscription message info: [`MessageInfo`] metadata plus the
/// sample's wire-level **attachment** bytes, borrowed for the callback
/// scope.
///
/// Surfaced on the generic (type-erased) subscription path
/// (`node.subscription(t).generic(..).message_info().build(cb)` — the
/// `FnMut(&[u8], &RawMessageInfo)` callback). The attachment carries
/// out-of-band tags such as the cross-RMW bridge's `bridge_origin`
/// (read via [`attachment`](Self::attachment) for echo suppression).
///
/// The `'a` lifetime ties the borrowed attachment to the dispatch call;
/// copy out what you need before the callback returns. Metadata
/// accessors delegate to the inner [`MessageInfo`]; on backends without
/// a combined raw+info+attachment take they read their defaults (the
/// attachment is always populated).
#[derive(Debug, Clone, Copy)]
pub struct RawMessageInfo<'a> {
    info: MessageInfo,
    attachment: &'a [u8],
}

impl<'a> RawMessageInfo<'a> {
    /// Build from an attachment slice (metadata defaulted).
    pub const fn new(attachment: &'a [u8]) -> Self {
        Self {
            info: MessageInfo::new(),
            attachment,
        }
    }

    /// Build from explicit metadata + attachment.
    pub const fn with_info(info: MessageInfo, attachment: &'a [u8]) -> Self {
        Self { info, attachment }
    }

    /// The sample's wire-level attachment bytes (empty if none).
    pub const fn attachment(&self) -> &'a [u8] {
        self.attachment
    }

    /// The underlying metadata.
    pub const fn info(&self) -> &MessageInfo {
        &self.info
    }

    /// Timestamp when the message was published.
    pub const fn source_timestamp(&self) -> Time {
        self.info.source_timestamp()
    }

    /// Timestamp when the message was received.
    pub const fn received_timestamp(&self) -> Time {
        self.info.received_timestamp()
    }

    /// Publisher's Global Identifier (GID).
    ///
    /// Same bound as [`MessageInfo::publisher_gid`]: 24 bytes wide, with the
    /// meaningful prefix decided by the backend and today never the whole
    /// array.
    pub const fn publisher_gid(&self) -> &[u8; PUBLISHER_GID_SIZE] {
        self.info.publisher_gid()
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

    /// The width is upstream's, and that is the whole point of it: measured in
    /// the `ros2` box, `/opt/ros/humble/include/rmw/rmw/types.h:42` reads
    /// `#define RMW_GID_STORAGE_SIZE 24u`, and our own ABI header takes the
    /// same number. A gid taken from a sample and a gid written into an
    /// `rmw_gid_t` are one type only while this holds.
    #[test]
    fn publisher_gid_is_upstreams_storage_size() {
        assert_eq!(PUBLISHER_GID_SIZE, 24);
    }

    /// zenoh's identity is 16 bytes and cannot move — that is
    /// `rmw_zenoh_cpp`'s `RMW_ATTACHMENT_SIZE` wire layout, whose reader
    /// rejects any other length. Padding is what lets it be stored at the
    /// upstream width without either side moving.
    #[test]
    fn a_narrow_backend_identity_zero_extends_into_the_tail() {
        let wire = [0xABu8; 16];
        let wide = pad_publisher_gid(&wire);
        assert_eq!(&wide[..16], &wire[..]);
        assert_eq!(&wide[16..], &[0u8; PUBLISHER_GID_SIZE - 16]);

        // Deterministic, so two samples from one publisher still compare equal
        // over the WHOLE array — the property `test_gid_consistency` reads.
        assert_eq!(pad_publisher_gid(&wire), wide);
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
