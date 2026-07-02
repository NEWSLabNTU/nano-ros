// nros message type - pure Rust, no_std compatible
// Package: unique_identifier_msgs
// Message: UUID

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// UUID message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UUID {
    pub uuid: [u8; 16],
}

impl Serialize for UUID {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        for item in &self.uuid {
            writer.write_u8(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for UUID {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            uuid: {
                let mut arr: [u8; 16] = Default::default();
                for i in 0..16 {
                    arr[i] = reader.read_u8()?;
                }
                arr
            },
        })
    }
}

impl RosMessage for UUID {
    const TYPE_NAME: &'static str = "unique_identifier_msgs::msg::dds_::UUID_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_UUID_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Uint8;
impl ::nros_serdes::Message for UUID {
    const TYPE_NAME: &'static str = "unique_identifier_msgs/msg/UUID";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "uuid",
            ty: ::nros_serdes::FieldType::Array(16, &FT_UUID_ELEM),
            offset: ::core::mem::offset_of!(UUID, uuid),
        },
];
}