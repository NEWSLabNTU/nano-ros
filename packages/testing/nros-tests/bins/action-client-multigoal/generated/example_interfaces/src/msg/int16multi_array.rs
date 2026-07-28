// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: Int16MultiArray

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Int16MultiArray message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int16MultiArray {
    pub layout: crate::msg::MultiArrayLayout,
    pub data: heapless::Vec<i16, 64>,
}

impl Serialize for Int16MultiArray {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.layout.serialize(writer)?;
        writer.write_u32(self.data.len() as u32)?;
        for item in &self.data {
            writer.write_i16(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for Int16MultiArray {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            layout: Deserialize::deserialize(reader)?,
            data: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i16()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for Int16MultiArray {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::Int16MultiArray_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_LAYOUT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::MultiArrayLayout as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::MultiArrayLayout as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_DATA_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int16;
impl ::nros_serdes::Message for Int16MultiArray {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Int16MultiArray";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "layout",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_LAYOUT),
            offset: ::core::mem::offset_of!(Int16MultiArray, layout),
        },
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DATA_ELEM),
            offset: ::core::mem::offset_of!(Int16MultiArray, data),
        },
];
}