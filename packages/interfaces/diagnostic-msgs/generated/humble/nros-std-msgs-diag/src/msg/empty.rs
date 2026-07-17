// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Empty

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Empty message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Empty {}

impl Serialize for Empty {
    // Empty message - no fields to serialize
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for Empty {
    // Empty message - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for Empty {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Empty_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Empty {
    const TYPE_NAME: &'static str = "std_msgs/msg/Empty";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}
