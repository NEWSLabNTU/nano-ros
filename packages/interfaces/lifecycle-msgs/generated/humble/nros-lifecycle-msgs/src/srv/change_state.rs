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
        self.transition.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for ChangeStateRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            transition: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for ChangeStateRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// ChangeState response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeStateResponse {
    pub success: bool,
}

impl Serialize for ChangeStateResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.success)?;
        Ok(())
    }
}

impl Deserialize for ChangeStateResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            success: reader.read_bool()?,
        })
    }
}

impl RosMessage for ChangeStateResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// ChangeState service definition
pub struct ChangeState;

impl RosService for ChangeState {
    type Request = ChangeStateRequest;
    type Reply = ChangeStateResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::ChangeState_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
