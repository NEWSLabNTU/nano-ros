// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: ChangeState

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ChangeState request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeStateRequest {
    pub transition: crate::msg::Transition,
}

impl Serialize for ChangeStateRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.transition.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ChangeStateRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            transition: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ChangeStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_NESTED_TRANSITION: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Transition as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Transition as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for ChangeStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/ChangeState_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "transition",
        ty: ::nros_serdes::FieldType::Nested(&REQ_NESTED_TRANSITION),
        offset: ::core::mem::offset_of!(ChangeStateRequest, transition),
    }];
}

/// ChangeState response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeStateResponse {
    pub success: bool,
}

impl Serialize for ChangeStateResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_bool(self.success)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ChangeStateResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            success: reader.read_bool()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ChangeStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

impl ::nros_serdes::Message for ChangeStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/ChangeState_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "success",
        ty: ::nros_serdes::FieldType::Bool,
        offset: ::core::mem::offset_of!(ChangeStateResponse, success),
    }];
}

/// ChangeState service definition
pub struct ChangeState;

impl RosService for ChangeState {
    type Request = ChangeStateRequest;
    type Reply = ChangeStateResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
