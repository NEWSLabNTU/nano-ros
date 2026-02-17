// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterValue

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ParameterValue message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterValue {
    pub type_: u8,
    pub bool_value: bool,
    pub integer_value: i64,
    pub double_value: f64,
    pub string_value: heapless::String<256>,
    pub byte_array_value: heapless::Vec<u8, 64>,
    pub bool_array_value: heapless::Vec<bool, 64>,
    pub integer_array_value: heapless::Vec<i64, 64>,
    pub double_array_value: heapless::Vec<f64, 64>,
    pub string_array_value: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for ParameterValue {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u8(self.type_)?;
        writer.write_bool(self.bool_value)?;
        writer.write_i64(self.integer_value)?;
        writer.write_f64(self.double_value)?;
        writer.write_string(self.string_value.as_str())?;
        writer.write_u32(self.byte_array_value.len() as u32)?;
        for item in &self.byte_array_value {
            writer.write_u8(*item)?;
        }
        writer.write_u32(self.bool_array_value.len() as u32)?;
        for item in &self.bool_array_value {
            writer.write_bool(*item)?;
        }
        writer.write_u32(self.integer_array_value.len() as u32)?;
        for item in &self.integer_array_value {
            writer.write_i64(*item)?;
        }
        writer.write_u32(self.double_array_value.len() as u32)?;
        for item in &self.double_array_value {
            writer.write_f64(*item)?;
        }
        writer.write_u32(self.string_array_value.len() as u32)?;
        for item in &self.string_array_value {
            writer.write_string(item.as_str())?;
        }
        Ok(())
    }
}

impl Deserialize for ParameterValue {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            type_: reader.read_u8()?,
            bool_value: reader.read_bool()?,
            integer_value: reader.read_i64()?,
            double_value: reader.read_f64()?,
            string_value: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            byte_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_u8()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            bool_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_bool()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            integer_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            double_array_value: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_f64()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            string_array_value: {
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

impl RosMessage for ParameterValue {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterValue_";
    const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
