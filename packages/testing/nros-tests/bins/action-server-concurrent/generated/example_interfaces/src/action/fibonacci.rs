// nros action type - pure Rust, no_std compatible
// Package: example_interfaces
// Action: Fibonacci

use nros_core::{RosMessage, RosAction, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

// ============================================================================
// Goal Message
// ============================================================================

/// Fibonacci goal message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FibonacciGoal {
    pub order: i32,
}

impl Serialize for FibonacciGoal {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.order)?;
        Ok(())
    }
}

impl Deserialize for FibonacciGoal {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            order: reader.read_i32()?,
        })
    }
}

impl RosMessage for FibonacciGoal {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Goal_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Goal) ──────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for FibonacciGoal {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_Goal";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "order",
            ty: ::nros_serdes::FieldType::Int32,
            offset: ::core::mem::offset_of!(FibonacciGoal, order),
        },
];
}

// ============================================================================
// Result Message
// ============================================================================

/// Fibonacci result message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FibonacciResult {
    pub sequence: heapless::Vec<i32, 64>,
}

impl Serialize for FibonacciResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.sequence.len() as u32)?;
        for item in &self.sequence {
            writer.write_i32(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for FibonacciResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sequence: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i32()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for FibonacciResult {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Result_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Result) ────────────────────

#[allow(non_upper_case_globals)]
pub const RESULT_FT_SEQUENCE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int32;
impl ::nros_serdes::Message for FibonacciResult {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_Result";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "sequence",
            ty: ::nros_serdes::FieldType::Sequence(&RESULT_FT_SEQUENCE_ELEM),
            offset: ::core::mem::offset_of!(FibonacciResult, sequence),
        },
];
}

// ============================================================================
// Feedback Message
// ============================================================================

/// Fibonacci feedback message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FibonacciFeedback {
    pub sequence: heapless::Vec<i32, 64>,
}

impl Serialize for FibonacciFeedback {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.sequence.len() as u32)?;
        for item in &self.sequence {
            writer.write_i32(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for FibonacciFeedback {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sequence: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i32()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for FibonacciFeedback {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Feedback_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Feedback) ──────────────────

#[allow(non_upper_case_globals)]
pub const FEEDBACK_FT_SEQUENCE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int32;
impl ::nros_serdes::Message for FibonacciFeedback {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_Feedback";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "sequence",
            ty: ::nros_serdes::FieldType::Sequence(&FEEDBACK_FT_SEQUENCE_ELEM),
            offset: ::core::mem::offset_of!(FibonacciFeedback, sequence),
        },
];
}

// ============================================================================
// Action Envelope Structs (rosidl wire convention — Phase 212.K.7.1.d)
// ============================================================================
//
// These five structs wrap the user-facing Goal / Result / Feedback for the
// action service-shape protocol. They mirror upstream `rosidl_generator_cpp`
// — `<A>__struct.hpp` exposes the same five names with the same field
// layout. Users normally interact with `<A>Goal/Result/Feedback`; the
// envelopes are used by the action plumbing layer (server-side
// `SendGoal_Request` decode, client-side `GetResult_Response` decode,
// feedback topic deliveries, …).
//
// `goal_id` is `unique_identifier_msgs::msg::UUID`, NOT a flat `[u8;16]`
// — `rosidl` always wraps the 16-byte UUID in a one-field struct so the
// CDR layout (sequence-of-uint8 fixed length 16) matches upstream's
// `unique_identifier_msgs/UUID.msg`.

// ── <A>_SendGoal_Request { goal_id: UUID, goal: <A>Goal } ───────────────────

/// Wire envelope for the `send_goal` request half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fibonacci_SendGoal_Request {
    pub goal_id: unique_identifier_msgs::msg::UUID,
    pub goal: FibonacciGoal,
}

impl Serialize for Fibonacci_SendGoal_Request {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_id.serialize(writer)?;
        self.goal.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Fibonacci_SendGoal_Request {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_id: Deserialize::deserialize(reader)?,
            goal: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Fibonacci_SendGoal_Request {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_SendGoal_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

#[allow(non_upper_case_globals)]
pub const SG_REQ_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const SG_REQ_NESTED_GOAL: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <FibonacciGoal as ::nros_serdes::Message>::TYPE_NAME,
    fields: <FibonacciGoal as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Fibonacci_SendGoal_Request {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_SendGoal_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&SG_REQ_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Request, goal_id),
        },
        ::nros_serdes::Field {
            name: "goal",
            ty: ::nros_serdes::FieldType::Nested(&SG_REQ_NESTED_GOAL),
            offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Request, goal),
        },
];
}

// ── <A>_SendGoal_Response { accepted: bool, stamp: Time } ───────────────────

/// Wire envelope for the `send_goal` response half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fibonacci_SendGoal_Response {
    pub accepted: bool,
    pub stamp: builtin_interfaces::msg::Time,
}

impl Serialize for Fibonacci_SendGoal_Response {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.accepted)?;
        self.stamp.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Fibonacci_SendGoal_Response {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            accepted: reader.read_bool()?,
            stamp: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Fibonacci_SendGoal_Response {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_SendGoal_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

#[allow(non_upper_case_globals)]
pub const SG_RESP_NESTED_STAMP: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Fibonacci_SendGoal_Response {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_SendGoal_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "accepted",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Response, accepted),
        },
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&SG_RESP_NESTED_STAMP),
            offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Response, stamp),
        },
];
}

// ── <A>_GetResult_Request { goal_id: UUID } ─────────────────────────────────

/// Wire envelope for the `get_result` request half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fibonacci_GetResult_Request {
    pub goal_id: unique_identifier_msgs::msg::UUID,
}

impl Serialize for Fibonacci_GetResult_Request {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_id.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Fibonacci_GetResult_Request {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_id: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Fibonacci_GetResult_Request {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_GetResult_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

#[allow(non_upper_case_globals)]
pub const GR_REQ_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Fibonacci_GetResult_Request {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_GetResult_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&GR_REQ_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Fibonacci_GetResult_Request, goal_id),
        },
];
}

// ── <A>_GetResult_Response { status: i8, result: <A>Result } ────────────────

/// Wire envelope for the `get_result` response half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fibonacci_GetResult_Response {
    pub status: i8,
    pub result: FibonacciResult,
}

impl Serialize for Fibonacci_GetResult_Response {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i8(self.status)?;
        self.result.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Fibonacci_GetResult_Response {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            status: reader.read_i8()?,
            result: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Fibonacci_GetResult_Response {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_GetResult_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

#[allow(non_upper_case_globals)]
pub const GR_RESP_NESTED_RESULT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <FibonacciResult as ::nros_serdes::Message>::TYPE_NAME,
    fields: <FibonacciResult as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Fibonacci_GetResult_Response {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_GetResult_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(Fibonacci_GetResult_Response, status),
        },
        ::nros_serdes::Field {
            name: "result",
            ty: ::nros_serdes::FieldType::Nested(&GR_RESP_NESTED_RESULT),
            offset: ::core::mem::offset_of!(Fibonacci_GetResult_Response, result),
        },
];
}

// ── <A>_FeedbackMessage { goal_id: UUID, feedback: <A>Feedback } ────────────

/// Wire envelope for the action feedback topic.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fibonacci_FeedbackMessage {
    pub goal_id: unique_identifier_msgs::msg::UUID,
    pub feedback: FibonacciFeedback,
}

impl Serialize for Fibonacci_FeedbackMessage {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.goal_id.serialize(writer)?;
        self.feedback.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Fibonacci_FeedbackMessage {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            goal_id: Deserialize::deserialize(reader)?,
            feedback: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Fibonacci_FeedbackMessage {
    const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_FeedbackMessage_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

#[allow(non_upper_case_globals)]
pub const FB_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FB_NESTED_FEEDBACK: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <FibonacciFeedback as ::nros_serdes::Message>::TYPE_NAME,
    fields: <FibonacciFeedback as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Fibonacci_FeedbackMessage {
    const TYPE_NAME: &'static str = "example_interfaces/action/Fibonacci_FeedbackMessage";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&FB_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Fibonacci_FeedbackMessage, goal_id),
        },
        ::nros_serdes::Field {
            name: "feedback",
            ty: ::nros_serdes::FieldType::Nested(&FB_NESTED_FEEDBACK),
            offset: ::core::mem::offset_of!(Fibonacci_FeedbackMessage, feedback),
        },
];
}

// ============================================================================
// Action Definition
// ============================================================================

/// Fibonacci action definition
pub struct Fibonacci;

impl RosAction for Fibonacci {
    type Goal = FibonacciGoal;
    type Result = FibonacciResult;
    type Feedback = FibonacciFeedback;

    type SendGoalRequest = Fibonacci_SendGoal_Request;
    type SendGoalResponse = Fibonacci_SendGoal_Response;
    type GetResultRequest = Fibonacci_GetResult_Request;
    type GetResultResponse = Fibonacci_GetResult_Response;
    type FeedbackMessage = Fibonacci_FeedbackMessage;

    const ACTION_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_";
    const ACTION_HASH: &'static str = "TypeHashNotSupported";

    // phase-244 E3 (RFC-0044) — register the fixed `action_msgs` protocol types
    // the cancel service + status publisher serialize. The 8 action envelopes
    // auto-register generically in nros-node; these three are not RosAction
    // associated types, so the generated impl registers them. No-op unless the
    // cyclonedds backend is selected (zenoh / xrce self-describe types at the
    // wire); gated by the crate's `rmw-cyclonedds` feature so non-cyclonedds /
    // no_std builds pull no std RMW dep. Replaces the per-example hand-rolled
    // `#[cfg(feature = "rmw-cyclonedds")] { … }` registration block.
    fn register_protocol_types() -> ::core::result::Result<(), ()> {
        #[cfg(feature = "rmw-cyclonedds")]
        {
            ::nros_rmw_cyclonedds::register::<::action_msgs::srv::CancelGoalRequest>()
                .map_err(|_| ())?;
            ::nros_rmw_cyclonedds::register::<::action_msgs::srv::CancelGoalResponse>()
                .map_err(|_| ())?;
            ::nros_rmw_cyclonedds::register::<::action_msgs::msg::GoalStatusArray>()
                .map_err(|_| ())?;
        }
        ::core::result::Result::Ok(())
    }
}