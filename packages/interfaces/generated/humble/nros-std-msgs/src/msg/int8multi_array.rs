// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Int8MultiArray

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Int8MultiArray message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int8MultiArray {
    pub layout: crate::msg::MultiArrayLayout,
    pub data: heapless::Vec<i8, 64>,
}

impl Serialize for Int8MultiArray {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.layout.serialize(writer)?;
        writer.write_u32(self.data.len() as u32)?;
        for item in &self.data {
            writer.write_i8(*item)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Int8MultiArray {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            layout: Deserialize::deserialize(reader)?,
            data: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i8()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Int8MultiArray {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int8MultiArray_";
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
pub const FT_DATA_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int8;
impl ::nros_serdes::Message for Int8MultiArray {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int8MultiArray";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "layout",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_LAYOUT),
            offset: ::core::mem::offset_of!(Int8MultiArray, layout),
        },
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Sequence(&FT_DATA_ELEM),
            offset: ::core::mem::offset_of!(Int8MultiArray, data),
        },
    ];
}
