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
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for GetAvailableTransitionsRequest {
    // Empty request - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for GetAvailableTransitionsRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetAvailableTransitions response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableTransitionsResponse {
    pub available_transitions: heapless::Vec<crate::msg::TransitionDescription, 64>,
}

impl Serialize for GetAvailableTransitionsResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.available_transitions.len() as u32)?;
        for item in &self.available_transitions {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for GetAvailableTransitionsResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            available_transitions: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for GetAvailableTransitionsResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetAvailableTransitions service definition
pub struct GetAvailableTransitions;

impl RosService for GetAvailableTransitions {
    type Request = GetAvailableTransitionsRequest;
    type Reply = GetAvailableTransitionsResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableTransitions_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
