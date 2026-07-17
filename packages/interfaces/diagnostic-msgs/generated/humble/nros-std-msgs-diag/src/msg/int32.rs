// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Int32

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Int32 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int32 {
    pub data: i32,
}

impl Serialize for Int32 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.data)?;
        Ok(())
    }
}

impl Deserialize for Int32 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i32()?,
        })
    }
}

impl RosMessage for Int32 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int32_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Int32 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Int32,
        offset: ::core::mem::offset_of!(Int32, data),
    }];
}
