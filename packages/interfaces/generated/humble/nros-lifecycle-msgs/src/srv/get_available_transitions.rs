// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: GetAvailableTransitions

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetAvailableTransitions request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableTransitionsRequest {}

impl Serialize for GetAvailableTransitionsRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetAvailableTransitionsRequest {
    // Empty request - no fields to deserialize
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for GetAvailableTransitionsRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for GetAvailableTransitionsRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetAvailableTransitions_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}

/// GetAvailableTransitions response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableTransitionsResponse {
    pub available_transitions: heapless::Vec<crate::msg::TransitionDescription, 64>,
}

impl Serialize for GetAvailableTransitionsResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.available_transitions.len() as u32)?;
        for item in &self.available_transitions {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetAvailableTransitionsResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            available_transitions: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for GetAvailableTransitionsResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_AVAILABLE_TRANSITIONS: ::nros_serdes::NestedType =
    ::nros_serdes::NestedType {
        type_name: <crate::msg::TransitionDescription as ::nros_serdes::Message>::TYPE_NAME,
        fields: <crate::msg::TransitionDescription as ::nros_serdes::Message>::FIELDS,
    };
#[allow(non_upper_case_globals)]
pub const RESP_FT_AVAILABLE_TRANSITIONS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_AVAILABLE_TRANSITIONS);
impl ::nros_serdes::Message for GetAvailableTransitionsResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetAvailableTransitions_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "available_transitions",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_AVAILABLE_TRANSITIONS_ELEM),
        offset: ::core::mem::offset_of!(GetAvailableTransitionsResponse, available_transitions),
    }];
}

/// GetAvailableTransitions service definition
pub struct GetAvailableTransitions;

impl RosService for GetAvailableTransitions {
    type Request = GetAvailableTransitionsRequest;
    type Reply = GetAvailableTransitionsResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
