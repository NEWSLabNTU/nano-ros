// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ListParametersResult

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ListParametersResult message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersResult {
    pub names: heapless::Vec<heapless::String<256>, 64>,
    pub prefixes: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for ListParametersResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        writer.write_u32(self.prefixes.len() as u32)?;
        for item in &self.prefixes {
            writer.write_string(item.as_str())?;
        }
        Ok(())
    }
}

impl Deserialize for ListParametersResult {
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
        })
    }
}

impl RosMessage for ListParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ListParametersResult_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
