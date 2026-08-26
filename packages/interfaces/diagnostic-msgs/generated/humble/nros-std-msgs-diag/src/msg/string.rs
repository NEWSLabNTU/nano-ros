// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: String

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// String message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct String {
    pub data: heapless::String<256>,
}

impl Serialize for String {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.data.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for String {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            data: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for String {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::String_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for String {
    const TYPE_NAME: &'static str = "std_msgs/msg/String";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::String,
        offset: ::core::mem::offset_of!(String, data),
    }];
}
