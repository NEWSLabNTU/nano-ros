// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Topic

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Topic message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Topic {
    pub name: heapless::String<256>,
    pub type_: crate::msg::InterfaceType,
    pub qos: crate::msg::QoSProfile,
}

impl Serialize for Topic {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        self.type_.serialize(writer)?;
        self.qos.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Topic {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            type_: Deserialize::deserialize(reader)?,
            qos: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Topic {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Topic_";
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
pub const NESTED_TYPE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::InterfaceType as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::InterfaceType as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_QOS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::QoSProfile as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::QoSProfile as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Topic {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Topic";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Topic, name),
        },
        ::nros_serdes::Field {
            name: "type",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_TYPE),
            offset: ::core::mem::offset_of!(Topic, type_),
        },
        ::nros_serdes::Field {
            name: "qos",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_QOS),
            offset: ::core::mem::offset_of!(Topic, qos),
        },
    ];
}
