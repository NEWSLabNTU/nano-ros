// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: UInt16

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// UInt16 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UInt16 {
    pub data: u16,
}

impl Serialize for UInt16 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u16(self.data)?;
        Ok(())
    }
}

impl Deserialize for UInt16 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_u16()?,
        })
    }
}

impl RosMessage for UInt16 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::UInt16_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for UInt16 {
    const TYPE_NAME: &'static str = "std_msgs/msg/UInt16";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Uint16,
        offset: ::core::mem::offset_of!(UInt16, data),
    }];
}
