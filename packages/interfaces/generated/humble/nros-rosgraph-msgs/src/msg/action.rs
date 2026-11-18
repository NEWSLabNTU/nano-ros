// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Action

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Action message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Action {
    pub name: heapless::String<256>,
    pub send_goal: crate::msg::Service,
    pub get_result: crate::msg::Service,
    pub cancel_goal: crate::msg::Service,
    pub feedback: crate::msg::Topic,
    pub status: crate::msg::Topic,
}

impl Serialize for Action {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        self.send_goal.serialize(writer)?;
        self.get_result.serialize(writer)?;
        self.cancel_goal.serialize(writer)?;
        self.feedback.serialize(writer)?;
        self.status.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Action {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            send_goal: Deserialize::deserialize(reader)?,
            get_result: Deserialize::deserialize(reader)?,
            cancel_goal: Deserialize::deserialize(reader)?,
            feedback: Deserialize::deserialize(reader)?,
            status: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Action {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Action_";
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
pub const NESTED_SEND_GOAL: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Service as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Service as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_GET_RESULT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Service as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Service as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_CANCEL_GOAL: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Service as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Service as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_FEEDBACK: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Topic as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Topic as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_STATUS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Topic as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Topic as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Action {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Action";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Action, name),
        },
        ::nros_serdes::Field {
            name: "send_goal",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_SEND_GOAL),
            offset: ::core::mem::offset_of!(Action, send_goal),
        },
        ::nros_serdes::Field {
            name: "get_result",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_GET_RESULT),
            offset: ::core::mem::offset_of!(Action, get_result),
        },
        ::nros_serdes::Field {
            name: "cancel_goal",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_CANCEL_GOAL),
            offset: ::core::mem::offset_of!(Action, cancel_goal),
        },
        ::nros_serdes::Field {
            name: "feedback",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_FEEDBACK),
            offset: ::core::mem::offset_of!(Action, feedback),
        },
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_STATUS),
            offset: ::core::mem::offset_of!(Action, status),
        },
    ];
}
