// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: TransitionEvent

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// TransitionEvent message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransitionEvent {
    pub timestamp: u64,
    pub transition: crate::msg::Transition,
    pub start_state: crate::msg::State,
    pub goal_state: crate::msg::State,
}

impl Serialize for TransitionEvent {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u64(self.timestamp)?;
        self.transition.serialize(writer)?;
        self.start_state.serialize(writer)?;
        self.goal_state.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for TransitionEvent {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            timestamp: reader.read_u64()?,
            transition: Deserialize::deserialize(reader)?,
            start_state: Deserialize::deserialize(reader)?,
            goal_state: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for TransitionEvent {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::TransitionEvent_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}