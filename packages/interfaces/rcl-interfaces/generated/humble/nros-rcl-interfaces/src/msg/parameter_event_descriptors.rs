// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterEventDescriptors

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterEventDescriptors message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterEventDescriptors {
    pub new_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
    pub changed_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
    pub deleted_parameters: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
}

impl Serialize for ParameterEventDescriptors {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.new_parameters.len() as u32)?;
        for item in &self.new_parameters {
            item.serialize(writer)?;
        }
        writer.write_u32(self.changed_parameters.len() as u32)?;
        for item in &self.changed_parameters {
            item.serialize(writer)?;
        }
        writer.write_u32(self.deleted_parameters.len() as u32)?;
        for item in &self.deleted_parameters {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for ParameterEventDescriptors {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            new_parameters: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            changed_parameters: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            deleted_parameters: {
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

impl RosMessage for ParameterEventDescriptors {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::msg::dds_::ParameterEventDescriptors_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}
