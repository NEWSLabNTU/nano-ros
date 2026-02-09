// nano-ros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: GetParameterTypes

use nano_ros_core::{Deserialize, RosMessage, RosService, Serialize};
use nano_ros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetParameterTypes request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParameterTypesRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for GetParameterTypesRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        Ok(())
    }
}

impl Deserialize for GetParameterTypesRequest {
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

impl RosMessage for GetParameterTypesRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_Request_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// GetParameterTypes response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParameterTypesResponse {
    pub types: heapless::Vec<u8, 64>,
}

impl Serialize for GetParameterTypesResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.types.len() as u32)?;
        for item in &self.types {
            writer.write_u8(*item)?;
        }
        Ok(())
    }
}

impl Deserialize for GetParameterTypesResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            types: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_u8()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for GetParameterTypesResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_Response_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// GetParameterTypes service definition
pub struct GetParameterTypes;

impl RosService for GetParameterTypes {
    type Request = GetParameterTypesRequest;
    type Reply = GetParameterTypesResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_";
    const SERVICE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
