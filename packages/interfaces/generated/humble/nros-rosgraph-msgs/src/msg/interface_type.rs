// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: InterfaceType

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// InterfaceType message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterfaceType {
    pub name: heapless::String<256>,
    pub hash: crate::msg::TypeHash,
}

impl Serialize for InterfaceType {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        self.hash.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for InterfaceType {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            hash: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for InterfaceType {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::InterfaceType_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // Issue 1179 -- the type's bound on `RosMessage`, the one trait every
    // backend's registration can read. Computed from the schema below, not
    // copied, so it cannot drift from it.
    const MAX_SERIALIZED_BOUND: Option<usize> = ::nros_serdes::size::max_serialized_bound::<Self>();
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_HASH: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::TypeHash as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::TypeHash as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for InterfaceType {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/InterfaceType";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(InterfaceType, name),
        },
        ::nros_serdes::Field {
            name: "hash",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_HASH),
            offset: ::core::mem::offset_of!(InterfaceType, hash),
        },
    ];
}
