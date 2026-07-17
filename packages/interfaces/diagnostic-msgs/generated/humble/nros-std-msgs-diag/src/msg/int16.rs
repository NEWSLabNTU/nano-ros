// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Int16

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Int16 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int16 {
    pub data: i16,
}

impl Serialize for Int16 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i16(self.data)?;
        Ok(())
    }
}

impl Deserialize for Int16 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i16()?,
        })
    }
}

impl RosMessage for Int16 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int16_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Int16 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int16";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Int16,
        offset: ::core::mem::offset_of!(Int16, data),
    }];
}
