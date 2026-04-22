// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: GetAvailableStates

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetAvailableStates request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableStatesRequest {}

impl Serialize for GetAvailableStatesRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for GetAvailableStatesRequest {
    // Empty request - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for GetAvailableStatesRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetAvailableStates response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableStatesResponse {
    pub available_states: heapless::Vec<crate::msg::State, 64>,
}

impl Serialize for GetAvailableStatesResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.available_states.len() as u32)?;
        for item in &self.available_states {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for GetAvailableStatesResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            available_states: {
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

impl RosMessage for GetAvailableStatesResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetAvailableStates service definition
pub struct GetAvailableStates;

impl RosService for GetAvailableStates {
    type Request = GetAvailableStatesRequest;
    type Reply = GetAvailableStatesResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
