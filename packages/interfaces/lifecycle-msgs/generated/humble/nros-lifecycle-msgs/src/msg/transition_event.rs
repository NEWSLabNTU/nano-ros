// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: TransitionEvent

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// TransitionEvent message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransitionEvent {
    pub timestamp: u64,
    pub transition: crate::msg::Transition,
    pub start_state: crate::msg::State,
    pub goal_state: crate::msg::State,
}

impl Serialize for TransitionEvent {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u64(self.timestamp)?;
        self.transition.serialize(writer)?;
        self.start_state.serialize(writer)?;
        self.goal_state.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for TransitionEvent {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            timestamp: reader.read_u64()?,
            transition: Deserialize::deserialize(reader)?,
            start_state: Deserialize::deserialize(reader)?,
            goal_state: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for TransitionEvent {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::TransitionEvent_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

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
impl ::nros_serdes::Message for TransitionEvent {
    const TYPE_NAME: &'static str = "lifecycle_msgs/msg/TransitionEvent";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "timestamp",
            ty: ::nros_serdes::FieldType::Uint64,
            offset: ::core::mem::offset_of!(TransitionEvent, timestamp),
        },
        ::nros_serdes::Field {
            name: "transition",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_TRANSITION),
            offset: ::core::mem::offset_of!(TransitionEvent, transition),
        },
        ::nros_serdes::Field {
            name: "start_state",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_START_STATE),
            offset: ::core::mem::offset_of!(TransitionEvent, start_state),
        },
        ::nros_serdes::Field {
            name: "goal_state",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_GOAL_STATE),
            offset: ::core::mem::offset_of!(TransitionEvent, goal_state),
        },
    ];
}
