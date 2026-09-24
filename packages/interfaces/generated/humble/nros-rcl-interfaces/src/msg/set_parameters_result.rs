// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: SetParametersResult

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParametersResult message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersResult {
    pub successful: bool,
    pub reason: heapless::String<256>,
}

impl Serialize for SetParametersResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_bool(self.successful)?;
        writer.write_string(self.reason.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SetParametersResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            successful: reader.read_bool()?,
            reason: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for SetParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::SetParametersResult_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for SetParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/SetParametersResult";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "successful",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(SetParametersResult, successful),
        },
        ::nros_serdes::Field {
            name: "reason",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(SetParametersResult, reason),
        },
    ];
}
