// nros message type - pure Rust, no_std compatible
// Package: builtin_interfaces
// Message: Time

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Time message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Time {
    pub sec: i32,
    pub nanosec: u32,
}

impl Serialize for Time {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.sec)?;
        writer.write_u32(self.nanosec)?;
        Ok(())
    }
}

impl Deserialize for Time {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sec: reader.read_i32()?,
            nanosec: reader.read_u32()?,
        })
    }
}

impl RosMessage for Time {
    const TYPE_NAME: &'static str = "builtin_interfaces::msg::dds_::Time_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Time {
    const TYPE_NAME: &'static str = "builtin_interfaces/msg/Time";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "sec",
            ty: ::nros_serdes::FieldType::Int32,
            offset: ::core::mem::offset_of!(Time, sec),
        },
        ::nros_serdes::Field {
            name: "nanosec",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(Time, nanosec),
        },
];
}