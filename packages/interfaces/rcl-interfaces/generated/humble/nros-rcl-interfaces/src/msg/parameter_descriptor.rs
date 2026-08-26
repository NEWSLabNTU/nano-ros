// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterDescriptor

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterDescriptor message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterDescriptor {
    pub name: heapless::String<256>,
    pub type_: u8,
    pub description: heapless::String<256>,
    pub additional_constraints: heapless::String<256>,
    pub read_only: bool,
    pub dynamic_typing: bool,
    pub floating_point_range: heapless::Vec<crate::msg::FloatingPointRange, 1>,
    pub integer_range: heapless::Vec<crate::msg::IntegerRange, 1>,
}

impl Serialize for ParameterDescriptor {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        writer.write_u8(self.type_)?;
        writer.write_string(self.description.as_str())?;
        writer.write_string(self.additional_constraints.as_str())?;
        writer.write_bool(self.read_only)?;
        writer.write_bool(self.dynamic_typing)?;
        writer.write_u32(self.floating_point_range.len() as u32)?;
        for item in &self.floating_point_range {
            item.serialize(writer)?;
        }
        writer.write_u32(self.integer_range.len() as u32)?;
        for item in &self.integer_range {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ParameterDescriptor {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            type_: reader.read_u8()?,
            description: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            additional_constraints: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            read_only: reader.read_bool()?,
            dynamic_typing: reader.read_bool()?,
            floating_point_range: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            integer_range: {
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

impl RosMessage for ParameterDescriptor {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterDescriptor_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_FLOATING_POINT_RANGE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::FloatingPointRange as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::FloatingPointRange as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_FLOATING_POINT_RANGE_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_FLOATING_POINT_RANGE);
#[allow(non_upper_case_globals)]
pub const NESTED_INTEGER_RANGE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::IntegerRange as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::IntegerRange as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_INTEGER_RANGE_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_INTEGER_RANGE);
impl ::nros_serdes::Message for ParameterDescriptor {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ParameterDescriptor";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ParameterDescriptor, name),
        },
        ::nros_serdes::Field {
            name: "type",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(ParameterDescriptor, type_),
        },
        ::nros_serdes::Field {
            name: "description",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ParameterDescriptor, description),
        },
        ::nros_serdes::Field {
            name: "additional_constraints",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ParameterDescriptor, additional_constraints),
        },
        ::nros_serdes::Field {
            name: "read_only",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(ParameterDescriptor, read_only),
        },
        ::nros_serdes::Field {
            name: "dynamic_typing",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(ParameterDescriptor, dynamic_typing),
        },
        ::nros_serdes::Field {
            name: "floating_point_range",
            ty: ::nros_serdes::FieldType::BoundedSequence(1, &FT_FLOATING_POINT_RANGE_ELEM),
            offset: ::core::mem::offset_of!(ParameterDescriptor, floating_point_range),
        },
        ::nros_serdes::Field {
            name: "integer_range",
            ty: ::nros_serdes::FieldType::BoundedSequence(1, &FT_INTEGER_RANGE_ELEM),
            offset: ::core::mem::offset_of!(ParameterDescriptor, integer_range),
        },
    ];
}
