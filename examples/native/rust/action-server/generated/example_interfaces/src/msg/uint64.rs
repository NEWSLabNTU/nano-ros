// nros message type - pure Rust, no_std compatible
// Package: example_interfaces
// Message: UInt64

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// UInt64 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UInt64 {
    pub data: u64,
}

impl Serialize for UInt64 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u64(self.data)?;
        Ok(())
    }
}

impl Deserialize for UInt64 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_u64()?,
        })
    }
}

impl RosMessage for UInt64 {
    const TYPE_NAME: &'static str = "example_interfaces::msg::dds_::UInt64_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}