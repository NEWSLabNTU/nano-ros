// nros message type - pure Rust, no_std compatible
// Package: action_msgs
// Message: GoalStatus

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};
pub const STATUS_UNKNOWN: i8 = 0;
pub const STATUS_ACCEPTED: i8 = 1;
pub const STATUS_EXECUTING: i8 = 2;
pub const STATUS_CANCELING: i8 = 3;
pub const STATUS_SUCCEEDED: i8 = 4;
pub const STATUS_CANCELED: i8 = 5;
pub const STATUS_ABORTED: i8 = 6;

/// GoalStatus message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalStatus {
    pub goal_info: crate::msg::GoalInfo,
    pub status: i8,
}

impl Serialize for GoalStatus {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_info.serialize(writer)?;
        writer.write_i8(self.status)?;
        Ok(())
    }
}

impl Deserialize for GoalStatus {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_info: Deserialize::deserialize(reader)?,
            status: reader.read_i8()?,
        })
    }
}

impl RosMessage for GoalStatus {
    const TYPE_NAME: &'static str = "action_msgs::msg::dds_::GoalStatus_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_GOAL_INFO: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::GoalInfo as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::GoalInfo as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for GoalStatus {
    const TYPE_NAME: &'static str = "action_msgs/msg/GoalStatus";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_info",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_GOAL_INFO),
            offset: ::core::mem::offset_of!(GoalStatus, goal_info),
        },
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(GoalStatus, status),
        },
];
}