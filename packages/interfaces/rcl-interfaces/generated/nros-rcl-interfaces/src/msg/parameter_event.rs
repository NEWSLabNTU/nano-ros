// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterEvent

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterEvent message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterEvent {
    pub stamp: nros_builtin_interfaces::msg::Time,
    pub node: heapless::String<256>,
    pub new_parameters: heapless::Vec<crate::msg::Parameter, 64>,
    pub changed_parameters: heapless::Vec<crate::msg::Parameter, 64>,
    pub deleted_parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for ParameterEvent {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.stamp.serialize(writer)?;
        writer.write_string(self.node.as_str())?;
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

impl Deserialize for ParameterEvent {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            stamp: Deserialize::deserialize(reader)?,
            node: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
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

impl RosMessage for ParameterEvent {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterEvent_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
