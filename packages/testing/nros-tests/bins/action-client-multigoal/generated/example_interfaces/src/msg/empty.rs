// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: Empty

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Empty message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Empty {
}

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
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::Empty_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Empty {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Empty";
    const FIELDS: &'static [::nros_serdes::Field] = &[
];
}