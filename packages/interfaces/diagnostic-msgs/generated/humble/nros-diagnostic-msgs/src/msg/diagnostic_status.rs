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
        writer.write_u8(self.level)?;
        writer.write_string(self.name.as_str())?;
        writer.write_string(self.message.as_str())?;
        writer.write_string(self.hardware_id.as_str())?;
        writer.write_u32(self.values.len() as u32)?;
        for item in &self.values {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for DiagnosticStatus {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
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
        })
    }
}

impl RosMessage for DiagnosticStatus {
    const TYPE_NAME: &'static str = "diagnostic_msgs::msg::dds_::DiagnosticStatus_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

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
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(DiagnosticStatus, name),
        },
        ::nros_serdes::Field {
            name: "message",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(DiagnosticStatus, message),
        },
        ::nros_serdes::Field {
            name: "hardware_id",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(DiagnosticStatus, hardware_id),
        },
        ::nros_serdes::Field {
            name: "values",
            ty: ::nros_serdes::FieldType::Sequence(&FT_VALUES_ELEM),
            offset: ::core::mem::offset_of!(DiagnosticStatus, values),
        },
    ];
}
