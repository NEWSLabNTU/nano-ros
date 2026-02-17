// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: ListParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const DEPTH_RECURSIVE: u64 = 0;

/// ListParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersRequest {
    pub prefixes: heapless::Vec<heapless::String<256>, 64>,
    pub depth: u64,
}

impl Serialize for ListParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.prefixes.len() as u32)?;
        for item in &self.prefixes {
            writer.write_string(item.as_str())?;
        }
        writer.write_u64(self.depth)?;
        Ok(())
    }
}

impl Deserialize for ListParametersRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            prefixes: {
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
            depth: reader.read_u64()?,
        })
    }
}

impl RosMessage for ListParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_Request_";
    const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// ListParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersResponse {
    pub result: crate::msg::ListParametersResult,
}

impl Serialize for ListParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.result.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for ListParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            result: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for ListParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_Response_";
    const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// ListParameters service definition
pub struct ListParameters;

impl RosService for ListParameters {
    type Request = ListParametersRequest;
    type Reply = ListParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_";
    const SERVICE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
