// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: Bool

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Bool message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bool {
    pub data: bool,
}

impl Serialize for Bool {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.data)?;
        Ok(())
    }
}

impl Deserialize for Bool {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_bool()?,
        })
    }
}

impl RosMessage for Bool {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::Bool_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Bool {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Bool";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(Bool, data),
        },
];
}