// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: IntegerRange

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// IntegerRange message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntegerRange {
    pub from_value: i64,
    pub to_value: i64,
    pub step: u64,
}

impl Serialize for IntegerRange {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i64(self.from_value)?;
        writer.write_i64(self.to_value)?;
        writer.write_u64(self.step)?;
        Ok(())
    }
}

impl Deserialize for IntegerRange {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            from_value: reader.read_i64()?,
            to_value: reader.read_i64()?,
            step: reader.read_u64()?,
        })
    }
}

impl RosMessage for IntegerRange {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::IntegerRange_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}
