// nros service type - pure Rust, no_std compatible
// Package: action_msgs
// Service: CancelGoal

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// CancelGoal request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CancelGoalRequest {
    pub goal_info: crate::msg::GoalInfo,
}

impl Serialize for CancelGoalRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_info.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for CancelGoalRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_info: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for CancelGoalRequest {
    const TYPE_NAME: &'static str = "action_msgs::srv::dds_::CancelGoal_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_NESTED_GOAL_INFO: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::GoalInfo as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::GoalInfo as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for CancelGoalRequest {
    const TYPE_NAME: &'static str = "action_msgs/srv/CancelGoal_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_info",
            ty: ::nros_serdes::FieldType::Nested(&REQ_NESTED_GOAL_INFO),
            offset: ::core::mem::offset_of!(CancelGoalRequest, goal_info),
        },
];
}
pub const ERROR_NONE: i8 = 0;
pub const ERROR_REJECTED: i8 = 1;
pub const ERROR_UNKNOWN_GOAL_ID: i8 = 2;
pub const ERROR_GOAL_TERMINATED: i8 = 3;

/// CancelGoal response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CancelGoalResponse {
    pub return_code: i8,
    pub goals_canceling: heapless::Vec<crate::msg::GoalInfo, 64>,
}

impl Serialize for CancelGoalResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i8(self.return_code)?;
        writer.write_u32(self.goals_canceling.len() as u32)?;
        for item in &self.goals_canceling {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for CancelGoalResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            return_code: reader.read_i8()?,
            goals_canceling: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for CancelGoalResponse {
    const TYPE_NAME: &'static str = "action_msgs::srv::dds_::CancelGoal_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_GOALS_CANCELING: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::GoalInfo as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::GoalInfo as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_GOALS_CANCELING_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&RESP_NESTED_GOALS_CANCELING);
impl ::nros_serdes::Message for CancelGoalResponse {
    const TYPE_NAME: &'static str = "action_msgs/srv/CancelGoal_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "return_code",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(CancelGoalResponse, return_code),
        },
        ::nros_serdes::Field {
            name: "goals_canceling",
            ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_GOALS_CANCELING_ELEM),
            offset: ::core::mem::offset_of!(CancelGoalResponse, goals_canceling),
        },
];
}

/// CancelGoal service definition
pub struct CancelGoal;

impl RosService for CancelGoal {
    type Request = CancelGoalRequest;
    type Reply = CancelGoalResponse;

    const SERVICE_NAME: &'static str = "action_msgs::srv::dds_::CancelGoal_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}