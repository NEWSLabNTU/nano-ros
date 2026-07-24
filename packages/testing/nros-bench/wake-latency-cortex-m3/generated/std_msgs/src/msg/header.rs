// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Header

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Header message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub stamp: builtin_interfaces::msg::Time,
    pub frame_id: heapless::String<256>,
}

impl Serialize for Header {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.stamp.serialize(writer)?;
        writer.write_string(self.frame_id.as_str())?;
        Ok(())
    }
}

impl Deserialize for Header {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            stamp: Deserialize::deserialize(reader)?,
            frame_id: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for Header {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Header_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // RFC-0052 W3a — Header/Time-leading type: `stamp.sec` at CDR byte
    // 4 (raw-buffer peek for on-target max_age monitors).
    const STAMP_OFFSET: Option<usize> = Some(4);
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_STAMP: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Header {
    const TYPE_NAME: &'static str = "std_msgs/msg/Header";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_STAMP),
            offset: ::core::mem::offset_of!(Header, stamp),
        },
        ::nros_serdes::Field {
            name: "frame_id",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Header, frame_id),
        },
];
}