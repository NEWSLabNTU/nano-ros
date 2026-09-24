// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Empty

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Empty message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Empty {}

impl Serialize for Empty {
    // Empty message — under XCDR2 an appendable struct still carries a DHEADER
    // (size 0); under XCDR1 this is a no-op (byte-identical: nothing written).
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Empty {
    // Empty message — read/skip the XCDR2 DHEADER (no-op under XCDR1).
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for Empty {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Empty_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for Empty {
    const TYPE_NAME: &'static str = "std_msgs/msg/Empty";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}
