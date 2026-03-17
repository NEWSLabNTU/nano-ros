// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: GetParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParametersRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for GetParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        Ok(())
    }
}

impl Deserialize for GetParametersRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            names: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    vec.push(
                        heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?,
                    )
                    .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for GetParametersRequest {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::GetParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParametersResponse {
    pub values: heapless::Vec<crate::msg::ParameterValue, 64>,
}

impl Serialize for GetParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.values.len() as u32)?;
        for item in &self.values {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for GetParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            values: {
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

impl RosMessage for GetParametersResponse {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::GetParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

/// GetParameters service definition
pub struct GetParameters;

impl RosService for GetParameters {
    type Request = GetParametersRequest;
    type Reply = GetParametersResponse;

    const SERVICE_NAME: &'static str = "nros_rcl_interfaces::srv::dds_::GetParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
