// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterType

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const PARAMETER_NOT_SET: u8 = 0;
pub const PARAMETER_BOOL: u8 = 1;
pub const PARAMETER_INTEGER: u8 = 2;
pub const PARAMETER_DOUBLE: u8 = 3;
pub const PARAMETER_STRING: u8 = 4;
pub const PARAMETER_BYTE_ARRAY: u8 = 5;
pub const PARAMETER_BOOL_ARRAY: u8 = 6;
pub const PARAMETER_INTEGER_ARRAY: u8 = 7;
pub const PARAMETER_DOUBLE_ARRAY: u8 = 8;
pub const PARAMETER_STRING_ARRAY: u8 = 9;

/// ParameterType message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterType {}

impl Serialize for ParameterType {
    // Empty message - no fields to serialize
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for ParameterType {
    // Empty message - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for ParameterType {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterType_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}
