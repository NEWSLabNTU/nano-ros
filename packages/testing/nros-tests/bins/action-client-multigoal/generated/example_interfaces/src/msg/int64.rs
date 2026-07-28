// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: Int64

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Int64 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int64 {
    pub data: i64,
}

impl Serialize for Int64 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i64(self.data)?;
        Ok(())
    }
}

impl Deserialize for Int64 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i64()?,
        })
    }
}

impl RosMessage for Int64 {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::Int64_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Int64 {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Int64";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(Int64, data),
        },
];
}