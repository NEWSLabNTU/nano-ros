// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Bool

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Bool message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bool {
    pub data: bool,
}

impl Serialize for Bool {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.data)?;
        Ok(())
    }
}

impl Deserialize for Bool {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_bool()?,
        })
    }
}

impl RosMessage for Bool {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Bool_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Bool {
    const TYPE_NAME: &'static str = "std_msgs/msg/Bool";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Bool,
        offset: ::core::mem::offset_of!(Bool, data),
    }];
}
