// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: UInt64

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// UInt64 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UInt64 {
    pub data: u64,
}

impl Serialize for UInt64 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u64(self.data)?;
        Ok(())
    }
}

impl Deserialize for UInt64 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_u64()?,
        })
    }
}

impl RosMessage for UInt64 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::UInt64_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for UInt64 {
    const TYPE_NAME: &'static str = "std_msgs/msg/UInt64";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Uint64,
            offset: ::core::mem::offset_of!(UInt64, data),
        },
];
}