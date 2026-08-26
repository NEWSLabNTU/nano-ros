// nros message type - pure Rust, no_std compatible
// Package: diagnostic_msgs
// Message: KeyValue

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// KeyValue message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyValue {
    pub key: heapless::String<32>,
    pub value: heapless::String<64>,
}

impl Serialize for KeyValue {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.key.as_str())?;
        writer.write_string(self.value.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for KeyValue {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            key: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            value: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for KeyValue {
    const TYPE_NAME: &'static str = "diagnostic_msgs::msg::dds_::KeyValue_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for KeyValue {
    const TYPE_NAME: &'static str = "diagnostic_msgs/msg/KeyValue";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "key",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(KeyValue, key),
        },
        ::nros_serdes::Field {
            name: "value",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(KeyValue, value),
        },
    ];
}
