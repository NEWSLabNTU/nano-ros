// nros message type - pure Rust, no_std compatible
// Package: action_msgs
// Message: GoalInfo

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// GoalInfo message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalInfo {
    pub goal_id: unique_identifier_msgs::msg::UUID,
    pub stamp: builtin_interfaces::msg::Time,
}

impl Serialize for GoalInfo {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_id.serialize(writer)?;
        self.stamp.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for GoalInfo {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_id: Deserialize::deserialize(reader)?,
            stamp: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for GoalInfo {
    const TYPE_NAME: &'static str = "action_msgs::msg::dds_::GoalInfo_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_STAMP: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for GoalInfo {
    const TYPE_NAME: &'static str = "action_msgs/msg/GoalInfo";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(GoalInfo, goal_id),
        },
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_STAMP),
            offset: ::core::mem::offset_of!(GoalInfo, stamp),
        },
];
}