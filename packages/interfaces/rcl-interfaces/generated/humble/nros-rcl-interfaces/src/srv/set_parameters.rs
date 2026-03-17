// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: SetParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersRequest {
    pub parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for SetParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.parameters.len() as u32)?;
        for item in &self.parameters {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for SetParametersRequest {
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

impl RosMessage for SetParametersRequest {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::SetParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// SetParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersResponse {
    pub results: heapless::Vec<crate::msg::SetParametersResult, 64>,
}

impl Serialize for SetParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.results.len() as u32)?;
        for item in &self.results {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for SetParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            results: {
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

impl RosMessage for SetParametersResponse {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::SetParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// SetParameters service definition
pub struct SetParameters;

impl RosService for SetParameters {
    type Request = SetParametersRequest;
    type Reply = SetParametersResponse;

    const SERVICE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::SetParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
