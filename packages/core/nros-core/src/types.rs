//! Core ROS type traits.
//!
//! [`RosMessage`] and [`RosService`] are implemented by code-generated message
//! and service types (via `cargo nano-ros generate`). Each type carries a
//! DDS-format type name and RIHS hash used for topic matching and type safety.

/// Trait for ROS message types
///
/// Identifies a ROS message type by its DDS type name and RIHS hash.
/// Serialization and deserialization are handled by separate traits:
/// - `nros_serdes::Serialize` — implemented by all message types
/// - `nros_serdes::Deserialize` — implemented by owned types
/// - `deserialize_borrowed()` — inherent method on borrowed types (zero-copy)
pub trait RosMessage: Sized {
    /// Full ROS type name in DDS format
    ///
    /// Example: `"std_msgs::msg::dds_::String_"`
    const TYPE_NAME: &'static str;

    /// RIHS (ROS Interface Hashing Standard) type hash
    ///
    /// Used for type validation between publishers and subscribers.
    /// Format: 64-character hex string (SHA-256)
    const TYPE_HASH: &'static str;
}

/// Trait for ROS service types
///
/// Associates request and reply message types with service metadata.
pub trait RosService {
    /// The request message type (services are always owned — needs Serialize + Deserialize)
    type Request: RosMessage + nros_serdes::Serialize + nros_serdes::Deserialize;

    /// The reply message type (services are always owned — needs Serialize + Deserialize)
    type Reply: RosMessage + nros_serdes::Serialize + nros_serdes::Deserialize;

    /// Full ROS service type name in DDS format
    ///
    /// Example: `"std_srvs::srv::dds_::Empty_"`
    const SERVICE_NAME: &'static str;

    /// RIHS type hash for the service
    const SERVICE_HASH: &'static str;
}
