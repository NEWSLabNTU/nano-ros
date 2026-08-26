// nros message type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Message: Transition

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const TRANSITION_CREATE: u8 = 0;
pub const TRANSITION_CONFIGURE: u8 = 1;
pub const TRANSITION_CLEANUP: u8 = 2;
pub const TRANSITION_ACTIVATE: u8 = 3;
pub const TRANSITION_DEACTIVATE: u8 = 4;
pub const TRANSITION_UNCONFIGURED_SHUTDOWN: u8 = 5;
pub const TRANSITION_INACTIVE_SHUTDOWN: u8 = 6;
pub const TRANSITION_ACTIVE_SHUTDOWN: u8 = 7;
pub const TRANSITION_DESTROY: u8 = 8;
pub const TRANSITION_ON_CONFIGURE_SUCCESS: u8 = 10;
pub const TRANSITION_ON_CONFIGURE_FAILURE: u8 = 11;
pub const TRANSITION_ON_CONFIGURE_ERROR: u8 = 12;
pub const TRANSITION_ON_CLEANUP_SUCCESS: u8 = 20;
pub const TRANSITION_ON_CLEANUP_FAILURE: u8 = 21;
pub const TRANSITION_ON_CLEANUP_ERROR: u8 = 22;
pub const TRANSITION_ON_ACTIVATE_SUCCESS: u8 = 30;
pub const TRANSITION_ON_ACTIVATE_FAILURE: u8 = 31;
pub const TRANSITION_ON_ACTIVATE_ERROR: u8 = 32;
pub const TRANSITION_ON_DEACTIVATE_SUCCESS: u8 = 40;
pub const TRANSITION_ON_DEACTIVATE_FAILURE: u8 = 41;
pub const TRANSITION_ON_DEACTIVATE_ERROR: u8 = 42;
pub const TRANSITION_ON_SHUTDOWN_SUCCESS: u8 = 50;
pub const TRANSITION_ON_SHUTDOWN_FAILURE: u8 = 51;
pub const TRANSITION_ON_SHUTDOWN_ERROR: u8 = 52;
pub const TRANSITION_ON_ERROR_SUCCESS: u8 = 60;
pub const TRANSITION_ON_ERROR_FAILURE: u8 = 61;
pub const TRANSITION_ON_ERROR_ERROR: u8 = 62;
pub const TRANSITION_CALLBACK_SUCCESS: u8 = 97;
pub const TRANSITION_CALLBACK_FAILURE: u8 = 98;
pub const TRANSITION_CALLBACK_ERROR: u8 = 99;

/// Transition message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transition {
    pub id: u8,
    pub label: heapless::String<256>,
}

impl Serialize for Transition {
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

impl Deserialize for Transition {
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

impl RosMessage for Transition {
    const TYPE_NAME: &'static str = "lifecycle_msgs::msg::dds_::Transition_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Transition {
    const TYPE_NAME: &'static str = "lifecycle_msgs/msg/Transition";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "id",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(Transition, id),
        },
        ::nros_serdes::Field {
            name: "label",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Transition, label),
        },
    ];
}
