// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterValue

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterValue message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterValue {
    pub type_: u8,
    pub bool_value: bool,
    pub integer_value: i64,
    pub double_value: f64,
    pub string_value: heapless::String<256>,
    pub byte_array_value: heapless::Vec<u8, 64>,
    pub bool_array_value: heapless::Vec<bool, 64>,
    pub integer_array_value: heapless::Vec<i64, 64>,
    pub double_array_value: heapless::Vec<f64, 64>,
    pub string_array_value: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for ParameterValue {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u8(self.type_)?;
        writer.write_bool(self.bool_value)?;
        writer.write_i64(self.integer_value)?;
        writer.write_f64(self.double_value)?;
        writer.write_string(self.string_value.as_str())?;
        writer.write_u32(self.byte_array_value.len() as u32)?;
        for item in &self.byte_array_value {
            writer.write_u8(*item)?;
        }
        writer.write_u32(self.bool_array_value.len() as u32)?;
        for item in &self.bool_array_value {
            writer.write_bool(*item)?;
        }
        writer.write_u32(self.integer_array_value.len() as u32)?;
        for item in &self.integer_array_value {
            writer.write_i64(*item)?;
        }
        writer.write_u32(self.double_array_value.len() as u32)?;
        for item in &self.double_array_value {
            writer.write_f64(*item)?;
        }
        writer.write_u32(self.string_array_value.len() as u32)?;
        for item in &self.string_array_value {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ParameterValue {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            type_: reader.read_u8()?,
            bool_value: reader.read_bool()?,
            integer_value: reader.read_i64()?,
            double_value: reader.read_f64()?,
            string_value: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            byte_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_u8()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            bool_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_bool()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            integer_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            double_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_f64()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            string_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem =
                        heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ParameterValue {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterValue_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_BYTE_ARRAY_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Uint8;
#[allow(non_upper_case_globals)]
pub const FT_BOOL_ARRAY_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Bool;
#[allow(non_upper_case_globals)]
pub const FT_INTEGER_ARRAY_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int64;
#[allow(non_upper_case_globals)]
pub const FT_DOUBLE_ARRAY_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Float64;
#[allow(non_upper_case_globals)]
pub const FT_STRING_ARRAY_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for ParameterValue {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ParameterValue";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "type",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(ParameterValue, type_),
        },
        ::nros_serdes::Field {
            name: "bool_value",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(ParameterValue, bool_value),
        },
        ::nros_serdes::Field {
            name: "integer_value",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(ParameterValue, integer_value),
        },
        ::nros_serdes::Field {
            name: "double_value",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(ParameterValue, double_value),
        },
        ::nros_serdes::Field {
            name: "string_value",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ParameterValue, string_value),
        },
        ::nros_serdes::Field {
            name: "byte_array_value",
            ty: ::nros_serdes::FieldType::Sequence(&FT_BYTE_ARRAY_VALUE_ELEM),
            offset: ::core::mem::offset_of!(ParameterValue, byte_array_value),
        },
        ::nros_serdes::Field {
            name: "bool_array_value",
            ty: ::nros_serdes::FieldType::Sequence(&FT_BOOL_ARRAY_VALUE_ELEM),
            offset: ::core::mem::offset_of!(ParameterValue, bool_array_value),
        },
        ::nros_serdes::Field {
            name: "integer_array_value",
            ty: ::nros_serdes::FieldType::Sequence(&FT_INTEGER_ARRAY_VALUE_ELEM),
            offset: ::core::mem::offset_of!(ParameterValue, integer_array_value),
        },
        ::nros_serdes::Field {
            name: "double_array_value",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DOUBLE_ARRAY_VALUE_ELEM),
            offset: ::core::mem::offset_of!(ParameterValue, double_array_value),
        },
        ::nros_serdes::Field {
            name: "string_array_value",
            ty: ::nros_serdes::FieldType::Sequence(&FT_STRING_ARRAY_VALUE_ELEM),
            offset: ::core::mem::offset_of!(ParameterValue, string_array_value),
        },
    ];
}
