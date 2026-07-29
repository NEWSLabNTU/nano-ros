// emit:ok
// nros message type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Message: Nested

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Nested message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Nested {
    pub one: crate::msg::Shapes,
    pub many: heapless::Vec<crate::msg::Shapes, 64>,
}

impl Serialize for Nested {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.one.serialize(writer)?;
        writer.write_u32(self.many.len() as u32)?;
        for item in &self.many {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Nested {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            one: Deserialize::deserialize(reader)?,
            many: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Nested {
    const TYPE_NAME: &'static str = "fingerprint-corpus::msg::dds_::Nested_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_ONE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Shapes as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Shapes as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_MANY: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Shapes as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Shapes as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_MANY_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&NESTED_MANY);
impl ::nros_serdes::Message for Nested {
    const TYPE_NAME: &'static str = "fingerprint-corpus/msg/Nested";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "one",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_ONE),
            offset: ::core::mem::offset_of!(Nested, one),
        },
        ::nros_serdes::Field {
            name: "many",
            ty: ::nros_serdes::FieldType::Sequence(&FT_MANY_ELEM),
            offset: ::core::mem::offset_of!(Nested, many),
        },
];
}