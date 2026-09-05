// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Clock

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Clock message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clock {
    pub clock: nros_builtin_interfaces_clock::msg::Time,
}

impl Serialize for Clock {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.clock.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Clock {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            clock: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Clock {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Clock_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // RFC-0052 W3a — Header/Time-leading type: `stamp.sec` at CDR byte
    // 4 (raw-buffer peek for on-target max_age monitors).
    const STAMP_OFFSET: Option<usize> = Some(4);
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_CLOCK: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_builtin_interfaces_clock::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces_clock::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Clock {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Clock";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "clock",
        ty: ::nros_serdes::FieldType::Nested(&NESTED_CLOCK),
        offset: ::core::mem::offset_of!(Clock, clock),
    }];
}
