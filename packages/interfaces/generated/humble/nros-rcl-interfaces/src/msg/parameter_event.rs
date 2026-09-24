// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterEvent

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterEvent message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterEvent {
    pub stamp: nros_builtin_interfaces::msg::Time,
    pub node: heapless::String<256>,
    pub new_parameters: heapless::Vec<crate::msg::Parameter, 64>,
    pub changed_parameters: heapless::Vec<crate::msg::Parameter, 64>,
    pub deleted_parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for ParameterEvent {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.stamp.serialize(writer)?;
        writer.write_string(self.node.as_str())?;
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

impl Deserialize for ParameterEvent {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            stamp: Deserialize::deserialize(reader)?,
            node: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
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

impl RosMessage for ParameterEvent {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterEvent_";
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
    type_name: <nros_builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_NEW_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Parameter as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Parameter as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_NEW_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_NEW_PARAMETERS);
#[allow(non_upper_case_globals)]
pub const NESTED_CHANGED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Parameter as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Parameter as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_CHANGED_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_CHANGED_PARAMETERS);
#[allow(non_upper_case_globals)]
pub const NESTED_DELETED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Parameter as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Parameter as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_DELETED_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_DELETED_PARAMETERS);
impl ::nros_serdes::Message for ParameterEvent {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ParameterEvent";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_STAMP),
            offset: ::core::mem::offset_of!(ParameterEvent, stamp),
        },
        ::nros_serdes::Field {
            name: "node",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ParameterEvent, node),
        },
        ::nros_serdes::Field {
            name: "new_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_NEW_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEvent, new_parameters),
        },
        ::nros_serdes::Field {
            name: "changed_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_CHANGED_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEvent, changed_parameters),
        },
        ::nros_serdes::Field {
            name: "deleted_parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DELETED_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(ParameterEvent, deleted_parameters),
        },
    ];
}
