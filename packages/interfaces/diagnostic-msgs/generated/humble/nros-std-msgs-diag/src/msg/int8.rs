// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Int8

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Int8 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int8 {
    pub data: i8,
}

impl Serialize for Int8 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i8(self.data)?;
        Ok(())
    }
}

impl Deserialize for Int8 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i8()?,
        })
    }
}

impl RosMessage for Int8 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int8_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Int8 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int8";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(Int8, data),
        },
];
}