//! Core ROS type traits.
//!
//! [`RosMessage`] and [`RosService`] are implemented by code-generated message
//! and service types (via `nros generate-rust`). Each type carries a
//! DDS-format type name and RIHS hash used for topic matching and type safety.

/// Trait for ROS message types
///
/// Identifies a ROS message type by its DDS type name and RIHS hash.
/// All message types implement `Serialize` and `Deserialize`.
pub trait RosMessage: Sized + nros_serdes::Serialize + nros_serdes::Deserialize {
    /// Full ROS type name in DDS format
    ///
    /// Example: `"std_msgs::msg::dds_::String_"`
    const TYPE_NAME: &'static str;

    /// RIHS (ROS Interface Hashing Standard) type hash
    ///
    /// Used for type validation between publishers and subscribers.
    /// Format: 64-character hex string (SHA-256)
    const TYPE_HASH: &'static str;

    /// RFC-0052 / phase-296 W3a — byte offset of `header.stamp.sec` within
    /// this type's serialized CDR payload (encapsulation header included),
    /// or `None` when the type has no leading `std_msgs/Header` /
    /// `builtin_interfaces/Time`. Codegen-const, never runtime
    /// introspection: CDR here is little-endian with a 4-byte encapsulation
    /// header and `Time { i32 sec; u32 nanosec }` is 4-byte aligned, so a
    /// Header-leading (or Time-leading) type carries `sec` at byte 4 and
    /// `nanosec` at byte 8. On-target `max_age` monitors peek these two
    /// words from the raw receive buffer before deserialization.
    const STAMP_OFFSET: Option<usize> = None;
}

/// Marker trait for a borrowed (zero-copy) ROS message family (RFC-0033
/// `borrowed` storage mode, issue 0007).
///
/// A `borrowed`-mode message is a *family* of types parameterized by the
/// receive-buffer lifetime — e.g. `struct Image<'a> { data: &'a [u8], … }`.
/// Rust cannot name such a family with a single type parameter, so codegen
/// emits a zero-sized marker (`struct ImageBorrow;`) implementing this trait
/// with the generic associated type [`View`](Self::View) bound to the
/// lifetime-carrying message. The executor monomorphizes the borrowed
/// subscription on the marker and reconstructs `View<'a>` per callback via
/// [`DeserializeBorrowed`](nros_serdes::DeserializeBorrowed).
///
/// The marker carries the same [`TYPE_NAME`](Self::TYPE_NAME) /
/// [`TYPE_HASH`](Self::TYPE_HASH) identity as the owned [`RosMessage`] for the
/// same `.msg`, so topic matching is identical.
pub trait BorrowedMessage {
    /// The lifetime-carrying borrowed view of the message, valid for the
    /// duration of a single subscription callback.
    type View<'a>: nros_serdes::DeserializeBorrowed<'a>;

    /// Full ROS type name in DDS format (matches the owned [`RosMessage`]).
    const TYPE_NAME: &'static str;

    /// RIHS type hash (matches the owned [`RosMessage`]).
    const TYPE_HASH: &'static str;
}

/// Trait for ROS service types
///
/// Associates request and reply message types with service metadata.
pub trait RosService {
    /// The request message type
    type Request: RosMessage;

    /// The reply message type
    type Reply: RosMessage;

    /// Full ROS service type name in DDS format
    ///
    /// Example: `"std_srvs::srv::dds_::Empty_"`
    const SERVICE_NAME: &'static str;

    /// RIHS type hash for the service
    const SERVICE_HASH: &'static str;
}
