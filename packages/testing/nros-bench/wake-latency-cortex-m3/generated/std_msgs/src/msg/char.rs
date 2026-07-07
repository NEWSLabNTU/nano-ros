// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Char

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Char message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Char {
    pub data: u8,
}

impl Serialize for Char {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u8(self.data)?;
        Ok(())
    }
}

impl Deserialize for Char {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_u8()?,
        })
    }
}

impl RosMessage for Char {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Char_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Char {
    const TYPE_NAME: &'static str = "std_msgs/msg/Char";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(Char, data),
        },
];
}