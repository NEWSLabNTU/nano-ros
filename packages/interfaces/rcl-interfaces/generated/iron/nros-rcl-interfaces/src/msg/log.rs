// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: Log

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const DEBUG: u8 = 10;
pub const INFO: u8 = 20;
pub const WARN: u8 = 30;
pub const ERROR: u8 = 40;
pub const FATAL: u8 = 50;

/// Log message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Log {
    pub stamp: nros_builtin_interfaces::msg::Time,
    pub level: u8,
    pub name: heapless::String<256>,
    pub msg: heapless::String<256>,
    pub file: heapless::String<256>,
    pub function: heapless::String<256>,
    pub line: u32,
}

impl Serialize for Log {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.stamp.serialize(writer)?;
        writer.write_u8(self.level)?;
        writer.write_string(self.name.as_str())?;
        writer.write_string(self.msg.as_str())?;
        writer.write_string(self.file.as_str())?;
        writer.write_string(self.function.as_str())?;
        writer.write_u32(self.line)?;
        Ok(())
    }
}

impl Deserialize for Log {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            stamp: Deserialize::deserialize(reader)?,
            level: reader.read_u8()?,
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            msg: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            file: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            function: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            line: reader.read_u32()?,
        })
    }
}

impl RosMessage for Log {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::Log_";
    const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
