// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterEventDescriptors

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterEventDescriptors message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterEventDescriptors {
    pub new_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
    pub changed_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
    pub deleted_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
}

impl Serialize for ParameterEventDescriptors {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.new_parameters.len() as u32)?;
        for item in &self.new_parameters {
            item.serialize(writer)?;
        }
        writer.write_u32(self.changed_parameters.len() as u32)?;
        for item in &self.changed_parameters {
            item.serialize(writer)?;
        }
        writer.write_u32(self.deleted_parameters.len() as u32)?;
        for item in &self.deleted_parameters {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ParameterEventDescriptors {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            new_parameters: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            changed_parameters: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            deleted_parameters: {
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

impl RosMessage for ParameterEventDescriptors {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterEventDescriptors_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_NEW_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_NEW_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_NEW_PARAMETERS);
#[allow(non_upper_case_globals)]
pub const NESTED_CHANGED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_CHANGED_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_CHANGED_PARAMETERS);
#[allow(non_upper_case_globals)]
pub const NESTED_DELETED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_DELETED_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_DELETED_PARAMETERS);
impl ::nros_serdes::Message for ParameterEventDescriptors {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ParameterEventDescriptors";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "new_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_NEW_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEventDescriptors, new_parameters),
        },
        ::nros_serdes::Field {
            name: "changed_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_CHANGED_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEventDescriptors, changed_parameters),
        },
        ::nros_serdes::Field {
            name: "deleted_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DELETED_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEventDescriptors, deleted_parameters),
        },
    ];
}
