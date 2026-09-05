// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Int8

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Int8 message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Int8 {
    pub data: i8,
}

impl Serialize for Int8 {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_i8(self.data)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Int8 {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            data: reader.read_i8()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Int8 {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int8_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Int8 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int8";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "data",
        ty: ::nros_serdes::FieldType::Int8,
        offset: ::core::mem::offset_of!(Int8, data),
    }];
}
