// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: SetParametersAtomically

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParametersAtomically request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersAtomicallyRequest {
    pub parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for SetParametersAtomicallyRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.parameters.len() as u32)?;
        for item in &self.parameters {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for SetParametersAtomicallyRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            parameters: {
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

impl RosMessage for SetParametersAtomicallyRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_Request_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// SetParametersAtomically response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersAtomicallyResponse {
    pub result: crate::msg::SetParametersResult,
}

impl Serialize for SetParametersAtomicallyResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.result.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for SetParametersAtomicallyResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            result: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for SetParametersAtomicallyResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_Response_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// SetParametersAtomically service definition
pub struct SetParametersAtomically;

impl RosService for SetParametersAtomically {
    type Request = SetParametersAtomicallyRequest;
    type Reply = SetParametersAtomicallyResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_";
    const SERVICE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
