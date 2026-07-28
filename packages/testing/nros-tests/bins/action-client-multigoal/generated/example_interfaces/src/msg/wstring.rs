// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: WString

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// WString message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WString {
    pub data: heapless::String<256>,
}

impl Serialize for WString {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_string(self.data.as_str())?;
        Ok(())
    }
}

impl Deserialize for WString {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for WString {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::WString_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for WString {
    const TYPE_NAME: &'static str = "example_interfaces/msg/WString";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::WString,
            offset: ::core::mem::offset_of!(WString, data),
        },
];
}