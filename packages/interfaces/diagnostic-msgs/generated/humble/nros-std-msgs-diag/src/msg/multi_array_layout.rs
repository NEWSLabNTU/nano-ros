// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: MultiArrayLayout

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// MultiArrayLayout message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MultiArrayLayout {
    pub dim: heapless::Vec<crate::msg::MultiArrayDimension, 64>,
    pub data_offset: u32,
}

impl Serialize for MultiArrayLayout {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.dim.len() as u32)?;
        for item in &self.dim {
            item.serialize(writer)?;
        }
        writer.write_u32(self.data_offset)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for MultiArrayLayout {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            dim: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            data_offset: reader.read_u32()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for MultiArrayLayout {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::MultiArrayLayout_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_DIM: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::MultiArrayDimension as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::MultiArrayDimension as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_DIM_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&NESTED_DIM);
impl ::nros_serdes::Message for MultiArrayLayout {
    const TYPE_NAME: &'static str = "std_msgs/msg/MultiArrayLayout";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "dim",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DIM_ELEM),
            offset: ::core::mem::offset_of!(MultiArrayLayout, dim),
        },
        ::nros_serdes::Field {
            name: "data_offset",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(MultiArrayLayout, data_offset),
        },
    ];
}
