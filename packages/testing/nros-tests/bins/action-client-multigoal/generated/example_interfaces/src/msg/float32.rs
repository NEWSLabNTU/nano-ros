// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: Float32

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Float32 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Float32 {
    pub data: f32,
}

impl Serialize for Float32 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_f32(self.data)?;
        Ok(())
    }
}

impl Deserialize for Float32 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_f32()?,
        })
    }
}

impl RosMessage for Float32 {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::Float32_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Float32 {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Float32";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(Float32, data),
        },
];
}