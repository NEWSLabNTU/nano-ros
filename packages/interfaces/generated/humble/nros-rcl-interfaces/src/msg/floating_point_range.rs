// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: FloatingPointRange

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// FloatingPointRange message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FloatingPointRange {
    pub from_value: f64,
    pub to_value: f64,
    pub step: f64,
}

impl Serialize for FloatingPointRange {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_f64(self.from_value)?;
        writer.write_f64(self.to_value)?;
        writer.write_f64(self.step)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for FloatingPointRange {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            from_value: reader.read_f64()?,
            to_value: reader.read_f64()?,
            step: reader.read_f64()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for FloatingPointRange {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::FloatingPointRange_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for FloatingPointRange {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/FloatingPointRange";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "from_value",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(FloatingPointRange, from_value),
        },
        ::nros_serdes::Field {
            name: "to_value",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(FloatingPointRange, to_value),
        },
        ::nros_serdes::Field {
            name: "step",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(FloatingPointRange, step),
        },
    ];
}
