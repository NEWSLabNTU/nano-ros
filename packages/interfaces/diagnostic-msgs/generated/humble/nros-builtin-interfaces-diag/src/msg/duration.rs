// nros message type - pure Rust, no_std compatible
// Package: builtin_interfaces
// Message: Duration

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Duration message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Duration {
    pub sec: i32,
    pub nanosec: u32,
}

impl Serialize for Duration {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_i32(self.sec)?;
        writer.write_u32(self.nanosec)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Duration {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            sec: reader.read_i32()?,
            nanosec: reader.read_u32()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Duration {
    const TYPE_NAME: &'static str = "builtin_interfaces::msg::dds_::Duration_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Duration {
    const TYPE_NAME: &'static str = "builtin_interfaces/msg/Duration";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "sec",
            ty: ::nros_serdes::FieldType::Int32,
            offset: ::core::mem::offset_of!(Duration, sec),
        },
        ::nros_serdes::Field {
            name: "nanosec",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(Duration, nanosec),
        },
    ];
}
