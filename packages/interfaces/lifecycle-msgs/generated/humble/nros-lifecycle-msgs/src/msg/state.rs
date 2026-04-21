// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: State

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};
pub const PRIMARY_STATE_UNKNOWN: u8 = 0;
pub const PRIMARY_STATE_UNCONFIGURED: u8 = 1;
pub const PRIMARY_STATE_INACTIVE: u8 = 2;
pub const PRIMARY_STATE_ACTIVE: u8 = 3;
pub const PRIMARY_STATE_FINALIZED: u8 = 4;
pub const TRANSITION_STATE_CONFIGURING: u8 = 10;
pub const TRANSITION_STATE_CLEANINGUP: u8 = 11;
pub const TRANSITION_STATE_SHUTTINGDOWN: u8 = 12;
pub const TRANSITION_STATE_ACTIVATING: u8 = 13;
pub const TRANSITION_STATE_DEACTIVATING: u8 = 14;
pub const TRANSITION_STATE_ERRORPROCESSING: u8 = 15;

/// State message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub id: u8,
    pub label: heapless::String<256>,
}

impl Serialize for State {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u8(self.id)?;
        writer.write_string(self.label.as_str())?;
        Ok(())
    }
}

impl Deserialize for State {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            id: reader.read_u8()?,
            label: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for State {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::State_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}