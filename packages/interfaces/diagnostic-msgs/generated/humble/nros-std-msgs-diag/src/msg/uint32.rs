// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: UInt32

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// UInt32 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UInt32 {
    pub data: u32,
}

impl Serialize for UInt32 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.data)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for UInt32 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            data: reader.read_u32()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for UInt32 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::UInt32_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for UInt32 {
    const TYPE_NAME: &'static str = "std_msgs/msg/UInt32";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Uint32,
        offset: ::core::mem::offset_of!(UInt32, data),
    }];
}
