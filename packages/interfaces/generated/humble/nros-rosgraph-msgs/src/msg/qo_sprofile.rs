// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: QoSProfile

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// QoSProfile message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QoSProfile {
    pub depth: u32,
    pub deadline: nros_builtin_interfaces::msg::Duration,
    pub lifespan: nros_builtin_interfaces::msg::Duration,
    pub history: u8,
    pub reliability: u8,
    pub durability: u8,
    pub liveliness: u8,
    pub liveliness_lease_duration: nros_builtin_interfaces::msg::Duration,
}

impl QoSProfile {
    pub const HISTORY_SYSTEM_DEFAULT: u8 = 0;
    pub const HISTORY_KEEP_LAST: u8 = 1;
    pub const HISTORY_KEEP_ALL: u8 = 2;
    pub const HISTORY_UNKNOWN: u8 = 3;
    pub const RELIABILITY_SYSTEM_DEFAULT: u8 = 0;
    pub const RELIABILITY_RELIABLE: u8 = 1;
    pub const RELIABILITY_BEST_EFFORT: u8 = 2;
    pub const RELIABILITY_UNKNOWN: u8 = 3;
    pub const RELIABILITY_BEST_AVAILABLE: u8 = 4;
    pub const DURABILITY_SYSTEM_DEFAULT: u8 = 0;
    pub const DURABILITY_TRANSIENT_LOCAL: u8 = 1;
    pub const DURABILITY_VOLATILE: u8 = 2;
    pub const DURABILITY_UNKNOWN: u8 = 3;
    pub const DURABILITY_BEST_AVAILABLE: u8 = 4;
    pub const LIVELINESS_SYSTEM_DEFAULT: u8 = 0;
    pub const LIVELINESS_AUTOMATIC: u8 = 1;
    pub const LIVELINESS_MANUAL_BY_TOPIC: u8 = 3;
    pub const LIVELINESS_UNKNOWN: u8 = 4;
    pub const LIVELINESS_BEST_AVAILABLE: u8 = 5;
}

impl Serialize for QoSProfile {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.depth)?;
        self.deadline.serialize(writer)?;
        self.lifespan.serialize(writer)?;
        writer.write_u8(self.history)?;
        writer.write_u8(self.reliability)?;
        writer.write_u8(self.durability)?;
        writer.write_u8(self.liveliness)?;
        self.liveliness_lease_duration.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for QoSProfile {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            depth: reader.read_u32()?,
            deadline: Deserialize::deserialize(reader)?,
            lifespan: Deserialize::deserialize(reader)?,
            history: reader.read_u8()?,
            reliability: reader.read_u8()?,
            durability: reader.read_u8()?,
            liveliness: reader.read_u8()?,
            liveliness_lease_duration: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for QoSProfile {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::QoSProfile_";
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
pub const NESTED_DEADLINE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_LIFESPAN: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_LIVELINESS_LEASE_DURATION: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces::msg::Duration as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for QoSProfile {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/QoSProfile";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "depth",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(QoSProfile, depth),
        },
        ::nros_serdes::Field {
            name: "deadline",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_DEADLINE),
            offset: ::core::mem::offset_of!(QoSProfile, deadline),
        },
        ::nros_serdes::Field {
            name: "lifespan",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_LIFESPAN),
            offset: ::core::mem::offset_of!(QoSProfile, lifespan),
        },
        ::nros_serdes::Field {
            name: "history",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(QoSProfile, history),
        },
        ::nros_serdes::Field {
            name: "reliability",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(QoSProfile, reliability),
        },
        ::nros_serdes::Field {
            name: "durability",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(QoSProfile, durability),
        },
        ::nros_serdes::Field {
            name: "liveliness",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(QoSProfile, liveliness),
        },
        ::nros_serdes::Field {
            name: "liveliness_lease_duration",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_LIVELINESS_LEASE_DURATION),
            offset: ::core::mem::offset_of!(QoSProfile, liveliness_lease_duration),
        },
    ];
}
