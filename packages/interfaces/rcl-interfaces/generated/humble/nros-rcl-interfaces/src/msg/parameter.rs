// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: Parameter

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Parameter message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parameter {
    pub name: heapless::String<256>,
    pub value: crate::msg::ParameterValue,
}

impl Serialize for Parameter {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        self.value.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Parameter {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            value: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Parameter {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::Parameter_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_VALUE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterValue as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterValue as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Parameter {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/Parameter";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Parameter, name),
        },
        ::nros_serdes::Field {
            name: "value",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_VALUE),
            offset: ::core::mem::offset_of!(Parameter, value),
        },
    ];
}
