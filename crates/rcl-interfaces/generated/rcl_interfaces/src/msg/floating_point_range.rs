// nano-ros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: FloatingPointRange

use nano_ros_core::{Deserialize, RosMessage, Serialize};
use nano_ros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// FloatingPointRange message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FloatingPointRange {
    pub from_value: f64,
    pub to_value: f64,
    pub step: f64,
}

impl Serialize for FloatingPointRange {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_f64(self.from_value)?;
        writer.write_f64(self.to_value)?;
        writer.write_f64(self.step)?;
        Ok(())
    }
}

impl Deserialize for FloatingPointRange {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            from_value: reader.read_f64()?,
            to_value: reader.read_f64()?,
            step: reader.read_f64()?,
        })
    }
}

impl RosMessage for FloatingPointRange {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::FloatingPointRange_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
