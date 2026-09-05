// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: IntegerRange

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// IntegerRange message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntegerRange {
    pub from_value: i64,
    pub to_value: i64,
    pub step: u64,
}

impl Serialize for IntegerRange {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_i64(self.from_value)?;
        writer.write_i64(self.to_value)?;
        writer.write_u64(self.step)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for IntegerRange {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            from_value: reader.read_i64()?,
            to_value: reader.read_i64()?,
            step: reader.read_u64()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for IntegerRange {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::IntegerRange_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for IntegerRange {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/IntegerRange";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "from_value",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(IntegerRange, from_value),
        },
        ::nros_serdes::Field {
            name: "to_value",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(IntegerRange, to_value),
        },
        ::nros_serdes::Field {
            name: "step",
            ty: ::nros_serdes::FieldType::Uint64,
            offset: ::core::mem::offset_of!(IntegerRange, step),
        },
    ];
}
