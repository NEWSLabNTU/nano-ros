// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: TransitionDescription

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// TransitionDescription message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransitionDescription {
    pub transition: crate::msg::Transition,
    pub start_state: crate::msg::State,
    pub goal_state: crate::msg::State,
}

impl Serialize for TransitionDescription {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.transition.serialize(writer)?;
        self.start_state.serialize(writer)?;
        self.goal_state.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for TransitionDescription {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            transition: Deserialize::deserialize(reader)?,
            start_state: Deserialize::deserialize(reader)?,
            goal_state: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for TransitionDescription {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::TransitionDescription_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_TRANSITION: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Transition as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Transition as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_START_STATE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::State as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::State as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_GOAL_STATE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::State as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::State as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for TransitionDescription {
    const TYPE_NAME: &'static str = "lifecycle_msgs/msg/TransitionDescription";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "transition",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_TRANSITION),
            offset: ::core::mem::offset_of!(TransitionDescription, transition),
        },
        ::nros_serdes::Field {
            name: "start_state",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_START_STATE),
            offset: ::core::mem::offset_of!(TransitionDescription, start_state),
        },
        ::nros_serdes::Field {
            name: "goal_state",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_GOAL_STATE),
            offset: ::core::mem::offset_of!(TransitionDescription, goal_state),
        },
    ];
}
