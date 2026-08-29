// emit:ok
// nros action type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Action: Probe

use nros_core::{RosMessage, RosAction, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

// ============================================================================
// Goal Message
// ============================================================================

/// Probe goal message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeGoal {
    pub waypoints: heapless::Vec<i64, 8>,
    pub name: heapless::String<256>,
}

impl Serialize for ProbeGoal {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.waypoints.len() as u32)?;
        for item in &self.waypoints {
            writer.write_i64(*item)?;
        }
        writer.write_string(self.name.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ProbeGoal {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            waypoints: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ProbeGoal {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_Goal_";
    const TYPE_HASH: &'static str = "fingerprint";
}

// ── nros_serdes::Message — runtime field schema (Goal) ──────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const GOAL_FT_WAYPOINTS_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int64;
impl ::nros_serdes::Message for ProbeGoal {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_Goal";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "waypoints",
            ty: ::nros_serdes::FieldType::Sequence(&GOAL_FT_WAYPOINTS_ELEM),
            offset: ::core::mem::offset_of!(ProbeGoal, waypoints),
        },
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ProbeGoal, name),
        },
];
}

// ============================================================================
// Result Message
// ============================================================================

/// Probe result message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeResult {
    pub total: i64,
    pub report: heapless::String<256>,
}

impl Serialize for ProbeResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_i64(self.total)?;
        writer.write_string(self.report.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ProbeResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            total: reader.read_i64()?,
            report: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ProbeResult {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_Result_";
    const TYPE_HASH: &'static str = "fingerprint";
}

// ── nros_serdes::Message — runtime field schema (Result) ────────────────────

impl ::nros_serdes::Message for ProbeResult {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_Result";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "total",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(ProbeResult, total),
        },
        ::nros_serdes::Field {
            name: "report",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ProbeResult, report),
        },
];
}

// ============================================================================
// Feedback Message
// ============================================================================

/// Probe feedback message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeFeedback {
    pub done: i64,
    pub stage: heapless::String<256>,
}

impl Serialize for ProbeFeedback {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_i64(self.done)?;
        writer.write_string(self.stage.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ProbeFeedback {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            done: reader.read_i64()?,
            stage: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ProbeFeedback {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_Feedback_";
    const TYPE_HASH: &'static str = "fingerprint";
}

// ── nros_serdes::Message — runtime field schema (Feedback) ──────────────────

impl ::nros_serdes::Message for ProbeFeedback {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_Feedback";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "done",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(ProbeFeedback, done),
        },
        ::nros_serdes::Field {
            name: "stage",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ProbeFeedback, stage),
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
pub struct Probe_SendGoal_Request {
    pub goal_id: unique_identifier_msgs::msg::UUID,
    pub goal: ProbeGoal,
}

impl Serialize for Probe_SendGoal_Request {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.goal_id.serialize(writer)?;
        self.goal.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Probe_SendGoal_Request {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            goal_id: Deserialize::deserialize(reader)?,
            goal: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Probe_SendGoal_Request {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_SendGoal_Request_";
    const TYPE_HASH: &'static str = "fingerprint";
}

#[allow(non_upper_case_globals)]
pub const SG_REQ_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const SG_REQ_NESTED_GOAL: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <ProbeGoal as ::nros_serdes::Message>::TYPE_NAME,
    fields: <ProbeGoal as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Probe_SendGoal_Request {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_SendGoal_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&SG_REQ_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Probe_SendGoal_Request, goal_id),
        },
        ::nros_serdes::Field {
            name: "goal",
            ty: ::nros_serdes::FieldType::Nested(&SG_REQ_NESTED_GOAL),
            offset: ::core::mem::offset_of!(Probe_SendGoal_Request, goal),
        },
];
}

// ── <A>_SendGoal_Response { accepted: bool, stamp: Time } ───────────────────

/// Wire envelope for the `send_goal` response half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe_SendGoal_Response {
    pub accepted: bool,
    pub stamp: builtin_interfaces::msg::Time,
}

impl Serialize for Probe_SendGoal_Response {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_bool(self.accepted)?;
        self.stamp.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Probe_SendGoal_Response {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            accepted: reader.read_bool()?,
            stamp: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Probe_SendGoal_Response {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_SendGoal_Response_";
    const TYPE_HASH: &'static str = "fingerprint";
}

#[allow(non_upper_case_globals)]
pub const SG_RESP_NESTED_STAMP: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Probe_SendGoal_Response {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_SendGoal_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "accepted",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(Probe_SendGoal_Response, accepted),
        },
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&SG_RESP_NESTED_STAMP),
            offset: ::core::mem::offset_of!(Probe_SendGoal_Response, stamp),
        },
];
}

// ── <A>_GetResult_Request { goal_id: UUID } ─────────────────────────────────

/// Wire envelope for the `get_result` request half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe_GetResult_Request {
    pub goal_id: unique_identifier_msgs::msg::UUID,
}

impl Serialize for Probe_GetResult_Request {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.goal_id.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Probe_GetResult_Request {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            goal_id: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Probe_GetResult_Request {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_GetResult_Request_";
    const TYPE_HASH: &'static str = "fingerprint";
}

#[allow(non_upper_case_globals)]
pub const GR_REQ_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Probe_GetResult_Request {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_GetResult_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&GR_REQ_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Probe_GetResult_Request, goal_id),
        },
];
}

// ── <A>_GetResult_Response { status: i8, result: <A>Result } ────────────────

/// Wire envelope for the `get_result` response half of the action service.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe_GetResult_Response {
    pub status: i8,
    pub result: ProbeResult,
}

impl Serialize for Probe_GetResult_Response {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_i8(self.status)?;
        self.result.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Probe_GetResult_Response {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            status: reader.read_i8()?,
            result: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Probe_GetResult_Response {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_GetResult_Response_";
    const TYPE_HASH: &'static str = "fingerprint";
}

#[allow(non_upper_case_globals)]
pub const GR_RESP_NESTED_RESULT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <ProbeResult as ::nros_serdes::Message>::TYPE_NAME,
    fields: <ProbeResult as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Probe_GetResult_Response {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_GetResult_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(Probe_GetResult_Response, status),
        },
        ::nros_serdes::Field {
            name: "result",
            ty: ::nros_serdes::FieldType::Nested(&GR_RESP_NESTED_RESULT),
            offset: ::core::mem::offset_of!(Probe_GetResult_Response, result),
        },
];
}

// ── <A>_FeedbackMessage { goal_id: UUID, feedback: <A>Feedback } ────────────

/// Wire envelope for the action feedback topic.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe_FeedbackMessage {
    pub goal_id: unique_identifier_msgs::msg::UUID,
    pub feedback: ProbeFeedback,
}

impl Serialize for Probe_FeedbackMessage {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.goal_id.serialize(writer)?;
        self.feedback.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Probe_FeedbackMessage {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            goal_id: Deserialize::deserialize(reader)?,
            feedback: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Probe_FeedbackMessage {
    const TYPE_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_FeedbackMessage_";
    const TYPE_HASH: &'static str = "fingerprint";
}

#[allow(non_upper_case_globals)]
pub const FB_NESTED_GOAL_ID: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME,
    fields: <unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FB_NESTED_FEEDBACK: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <ProbeFeedback as ::nros_serdes::Message>::TYPE_NAME,
    fields: <ProbeFeedback as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Probe_FeedbackMessage {
    const TYPE_NAME: &'static str = "fingerprint-corpus/action/Probe_FeedbackMessage";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "goal_id",
            ty: ::nros_serdes::FieldType::Nested(&FB_NESTED_GOAL_ID),
            offset: ::core::mem::offset_of!(Probe_FeedbackMessage, goal_id),
        },
        ::nros_serdes::Field {
            name: "feedback",
            ty: ::nros_serdes::FieldType::Nested(&FB_NESTED_FEEDBACK),
            offset: ::core::mem::offset_of!(Probe_FeedbackMessage, feedback),
        },
];
}

// ============================================================================
// Action Definition
// ============================================================================

/// Probe action definition
pub struct Probe;

impl RosAction for Probe {
    type Goal = ProbeGoal;
    type Result = ProbeResult;
    type Feedback = ProbeFeedback;

    type SendGoalRequest = Probe_SendGoal_Request;
    type SendGoalResponse = Probe_SendGoal_Response;
    type GetResultRequest = Probe_GetResult_Request;
    type GetResultResponse = Probe_GetResult_Response;
    type FeedbackMessage = Probe_FeedbackMessage;

    const ACTION_NAME: &'static str = "fingerprint-corpus::action::dds_::Probe_";
    const ACTION_HASH: &'static str = "fingerprint";
    // Issue #0292 — the SendGoal / GetResult SERVICE hashes (distinct from the
    // action hash) that a stock rmw_zenoh_cpp peer matches on in the service
    // keyexpr. Without these a ROS 2 client's send_goal/get_result queries miss
    // a nano-ros action server's queryables.
    const SEND_GOAL_SERVICE_HASH: &'static str = "fingerprint";
    const GET_RESULT_SERVICE_HASH: &'static str = "fingerprint";

    // phase-244 E3 (RFC-0044) / issue #234 — register the fixed `action_msgs`
    // protocol types the cancel service + status publisher serialize
    // (`CancelGoal_{Request,Response}`, `GoalStatusArray`). The 8 action
    // envelopes auto-register generically in nros-node via `register_type::<M>()`;
    // these three are not `RosAction` associated types (they live in
    // `action_msgs`, which `nros-core` cannot name), so the generated impl
    // registers them here through the SAME generic
    // `nros_rmw::register_type_descriptor` seam.
    //
    // No `#[cfg(feature = "rmw-cyclonedds")]` gate and no named-backend
    // dependency (issue #60): the seam is a no-op when no descriptor-needing
    // backend installed a registrar (zenoh / xrce self-describe types at the
    // wire), and forwards the schema to Cyclone DDS's runtime type registry
    // when it is linked. The pre-#234 body called
    // `nros_rmw_cyclonedds::register::<M>()` behind this crate's own
    // `rmw-cyclonedds` feature — which the standard example build never turned
    // on, so the block compiled out and the CancelGoal / GoalStatusArray
    // descriptors were never built, failing the cancel-service / status-publisher
    // sub-creates with `ActionCreationFailed`.
    fn register_protocol_types() -> ::core::result::Result<(), ()> {
        ::nros_rmw::register_type_descriptor(
            <::action_msgs::srv::CancelGoalRequest as ::nros_serdes::Message>::TYPE_NAME,
            <::action_msgs::srv::CancelGoalRequest as ::nros_serdes::Message>::FIELDS,
        )
        .map_err(|_| ())?;
        ::nros_rmw::register_type_descriptor(
            <::action_msgs::srv::CancelGoalResponse as ::nros_serdes::Message>::TYPE_NAME,
            <::action_msgs::srv::CancelGoalResponse as ::nros_serdes::Message>::FIELDS,
        )
        .map_err(|_| ())?;
        ::nros_rmw::register_type_descriptor(
            <::action_msgs::msg::GoalStatusArray as ::nros_serdes::Message>::TYPE_NAME,
            <::action_msgs::msg::GoalStatusArray as ::nros_serdes::Message>::FIELDS,
        )
        .map_err(|_| ())?;
        ::core::result::Result::Ok(())
    }
}

// ── RFC-0033 `borrowed` (issue 0346) ────────────────────────────────────────
// Zero-copy view of the goal payload: fields marked `mode = "borrowed"`
// slice directly into the raw buffer the service/action callback hands you
// (`request_data`/`response`/feedback bytes), other fields are copied. Valid
// only while that buffer lives — copy out anything you retain.
/// Borrowed (zero-copy) view of [`ProbeGoal`].
pub struct ProbeGoalView<'a> {
    pub waypoints: nros_core::LeSliceView<'a, i64>,
    pub name: heapless::String<256>,
}

impl<'a> nros_core::DeserializeBorrowed<'a> for ProbeGoalView<'a> {
    fn deserialize_view(reader: &mut CdrReader<'a>) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            waypoints: reader.read_le_slice::<i64>()?,
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}