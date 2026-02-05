// nano-ros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: DescribeParameters

use nano_ros_core::{Deserialize, RosMessage, RosService, Serialize};
use nano_ros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// DescribeParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescribeParametersRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for DescribeParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        Ok(())
    }
}

impl Deserialize for DescribeParametersRequest {
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

impl RosMessage for DescribeParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_Request_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// DescribeParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescribeParametersResponse {
    pub descriptors: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
}

impl Serialize for DescribeParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.descriptors.len() as u32)?;
        for item in &self.descriptors {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for DescribeParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            descriptors: {
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

impl RosMessage for DescribeParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_Response_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

/// DescribeParameters service definition
pub struct DescribeParameters;

impl RosService for DescribeParameters {
    type Request = DescribeParametersRequest;
    type Reply = DescribeParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_";
    const SERVICE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
