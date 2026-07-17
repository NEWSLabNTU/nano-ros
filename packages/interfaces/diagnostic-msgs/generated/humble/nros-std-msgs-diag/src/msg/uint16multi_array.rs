// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: UInt16MultiArray

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// UInt16MultiArray message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UInt16MultiArray {
    pub layout: crate::msg::MultiArrayLayout,
    pub data: heapless::Vec<u16, 64>,
}

impl Serialize for UInt16MultiArray {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.layout.serialize(writer)?;
        writer.write_u32(self.data.len() as u32)?;
        for item in &self.data {
            writer.write_u16(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for UInt16MultiArray {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            layout: Deserialize::deserialize(reader)?,
            data: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_u16()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for UInt16MultiArray {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::UInt16MultiArray_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_LAYOUT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::MultiArrayLayout as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::MultiArrayLayout as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_DATA_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Uint16;
impl ::nros_serdes::Message for UInt16MultiArray {
    const TYPE_NAME: &'static str = "std_msgs/msg/UInt16MultiArray";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "layout",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_LAYOUT),
            offset: ::core::mem::offset_of!(UInt16MultiArray, layout),
        },
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DATA_ELEM),
            offset: ::core::mem::offset_of!(UInt16MultiArray, data),
        },
    ];
}
