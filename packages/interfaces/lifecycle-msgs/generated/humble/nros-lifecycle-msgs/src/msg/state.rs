// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: State

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
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
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u8(self.id)?;
        writer.write_string(self.label.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for State {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            id: reader.read_u8()?,
            label: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for State {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::State_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for State {
    const TYPE_NAME: &'static str = "lifecycle_msgs/msg/State";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "id",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(State, id),
        },
        ::nros_serdes::Field {
            name: "label",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(State, label),
        },
    ];
}
