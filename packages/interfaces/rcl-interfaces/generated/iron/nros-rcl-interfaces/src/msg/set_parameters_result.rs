// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: SetParametersResult

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParametersResult message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersResult {
    pub successful: bool,
    pub reason: heapless::String<256>,
}

impl Serialize for SetParametersResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.successful)?;
        writer.write_string(self.reason.as_str())?;
        Ok(())
    }
}

impl Deserialize for SetParametersResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            successful: reader.read_bool()?,
            reason: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for SetParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::SetParametersResult_";
    const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
