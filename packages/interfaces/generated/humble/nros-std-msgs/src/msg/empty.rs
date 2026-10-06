// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: Empty

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Empty message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Empty {}

impl Serialize for Empty {
    // Empty message — rosidl's one padding member, inside the XCDR2 DHEADER
    // (a no-op under XCDR1).
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        // Issue 1293 — rosidl's `structure_needs_at_least_one_member`: every
        // stock typesupport writes it and refuses a payload without it.
        writer.write_u8(0)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Empty {
    // Empty message — the padding member, inside the XCDR2 DHEADER.
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        // Issue 1293 — the padding byte every stock peer sends.
        let _ = reader.read_u8()?;
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
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: ::nros_serdes::schema::EMPTY_STRUCT_MEMBER,
        ty: ::nros_serdes::FieldType::Uint8,
        offset: 0,
    }];
}
