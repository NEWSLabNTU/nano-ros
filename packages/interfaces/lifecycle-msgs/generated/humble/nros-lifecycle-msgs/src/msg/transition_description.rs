// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: TransitionDescription

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// TransitionDescription message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransitionDescription {
    pub transition: crate::msg::Transition,
    pub start_state: crate::msg::State,
    pub goal_state: crate::msg::State,
}

impl Serialize for TransitionDescription {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        self.transition.serialize(writer)?;
        self.start_state.serialize(writer)?;
        self.goal_state.serialize(writer)?;
        Ok(())
    }
}

impl Deserialize for TransitionDescription {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            transition: Deserialize::deserialize(reader)?,
            start_state: Deserialize::deserialize(reader)?,
            goal_state: Deserialize::deserialize(reader)?,
        })
    }
}

impl RosMessage for TransitionDescription {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::TransitionDescription_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}
