// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: Parameter

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Parameter message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parameter {
    pub name: heapless::String<256>,
    pub value: crate::msg::ParameterValue,
}

impl Serialize for Parameter {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_string(self.name.as_str())?;
        self.value.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for Parameter {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            value: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for Parameter {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::Parameter_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
