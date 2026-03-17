// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterDescriptor

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterDescriptor message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterDescriptor {
    pub name: heapless::String<256>,
    pub type_: u8,
    pub description: heapless::String<256>,
    pub additional_constraints: heapless::String<256>,
    pub read_only: bool,
    pub dynamic_typing: bool,
    pub floating_point_range: heapless::Vec<crate::msg::FloatingPointRange, 1>,
    pub integer_range: heapless::Vec<crate::msg::IntegerRange, 1>,
}

impl Serialize for ParameterDescriptor {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_string(self.name.as_str())?;
        writer.write_u8(self.type_)?;
        writer.write_string(self.description.as_str())?;
        writer.write_string(self.additional_constraints.as_str())?;
        writer.write_bool(self.read_only)?;
        writer.write_bool(self.dynamic_typing)?;
        writer.write_u32(self.floating_point_range.len() as u32)?;
        for item in &self.floating_point_range {
            item.serialize(writer)?;
        }
        writer.write_u32(self.integer_range.len() as u32)?;
        for item in &self.integer_range {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for ParameterDescriptor {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            type_: reader.read_u8()?,
            description: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            additional_constraints: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            read_only: reader.read_bool()?,
            dynamic_typing: reader.read_bool()?,
            floating_point_range: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            integer_range: {
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

impl RosMessage for ParameterDescriptor {
    const TYPE_NAME: &'static str = "nros_rcl_interfaces::msg::dds_::ParameterDescriptor_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}
