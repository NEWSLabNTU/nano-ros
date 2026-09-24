// nros message type - pure Rust, no_std compatible
// Package: diagnostic_msgs
// Message: DiagnosticStatus

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const OK: u8 = 0;
pub const WARN: u8 = 1;
pub const ERROR: u8 = 2;
pub const STALE: u8 = 3;

/// DiagnosticStatus message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiagnosticStatus {
    pub level: u8,
    pub name: heapless::String<64>,
    pub message: heapless::String<128>,
    pub hardware_id: heapless::String<96>,
    pub values: heapless::Vec<crate::msg::KeyValue, 8>,
}

impl Serialize for DiagnosticStatus {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u8(self.level)?;
        writer.write_string(self.name.as_str())?;
        writer.write_string(self.message.as_str())?;
        writer.write_string(self.hardware_id.as_str())?;
        writer.write_u32(self.values.len() as u32)?;
        for item in &self.values {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for DiagnosticStatus {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            level: reader.read_u8()?,
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            message: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            hardware_id: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            values: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for DiagnosticStatus {
    const TYPE_NAME: &'static str = "diagnostic_msgs::msg::dds_::DiagnosticStatus_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_VALUES: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::KeyValue as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::KeyValue as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_VALUES_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_VALUES);
impl ::nros_serdes::Message for DiagnosticStatus {
    const TYPE_NAME: &'static str = "diagnostic_msgs/msg/DiagnosticStatus";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "level",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(DiagnosticStatus, level),
        },
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::BoundedString(64),
            offset: ::core::mem::offset_of!(DiagnosticStatus, name),
        },
        ::nros_serdes::Field {
            name: "message",
            ty: ::nros_serdes::FieldType::BoundedString(128),
            offset: ::core::mem::offset_of!(DiagnosticStatus, message),
        },
        ::nros_serdes::Field {
            name: "hardware_id",
            ty: ::nros_serdes::FieldType::BoundedString(96),
            offset: ::core::mem::offset_of!(DiagnosticStatus, hardware_id),
        },
        ::nros_serdes::Field {
            name: "values",
            ty: ::nros_serdes::FieldType::BoundedSequence(8, &FT_VALUES_ELEM),
            offset: ::core::mem::offset_of!(DiagnosticStatus, values),
        },
    ];
}
