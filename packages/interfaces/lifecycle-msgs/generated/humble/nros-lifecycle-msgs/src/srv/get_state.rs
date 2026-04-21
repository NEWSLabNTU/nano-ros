// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: GetState

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// GetState request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetStateRequest {
}

impl Serialize for GetStateRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for GetStateRequest {
    // Empty request - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for GetStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetState response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetStateResponse {
    pub current_state: crate::msg::State,
}

impl Serialize for GetStateResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.current_state.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for GetStateResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            current_state: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for GetStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetState service definition
pub struct GetState;

impl RosService for GetState {
    type Request = GetStateRequest;
    type Reply = GetStateResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetState_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}