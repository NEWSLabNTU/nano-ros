// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: GetState

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetState request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetStateRequest {}

impl Serialize for GetStateRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetStateRequest {
    // Empty request - no fields to deserialize
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for GetStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for GetStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetState_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}

/// GetState response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetStateResponse {
    pub current_state: crate::msg::State,
}

impl Serialize for GetStateResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.current_state.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetStateResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            current_state: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for GetStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_CURRENT_STATE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::State as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::State as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for GetStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetState_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "current_state",
        ty: ::nros_serdes::FieldType::Nested(&RESP_NESTED_CURRENT_STATE),
        offset: ::core::mem::offset_of!(GetStateResponse, current_state),
    }];
}

/// GetState service definition
pub struct GetState;

impl RosService for GetState {
    type Request = GetStateRequest;
    type Reply = GetStateResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
