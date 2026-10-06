// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ParameterType

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const PARAMETER_NOT_SET: u8 = 0;
pub const PARAMETER_BOOL: u8 = 1;
pub const PARAMETER_INTEGER: u8 = 2;
pub const PARAMETER_DOUBLE: u8 = 3;
pub const PARAMETER_STRING: u8 = 4;
pub const PARAMETER_BYTE_ARRAY: u8 = 5;
pub const PARAMETER_BOOL_ARRAY: u8 = 6;
pub const PARAMETER_INTEGER_ARRAY: u8 = 7;
pub const PARAMETER_DOUBLE_ARRAY: u8 = 8;
pub const PARAMETER_STRING_ARRAY: u8 = 9;

/// ParameterType message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterType {}

impl Serialize for ParameterType {
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

impl Deserialize for ParameterType {
    // Empty message — the padding member, inside the XCDR2 DHEADER.
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        // Issue 1293 — the padding byte every stock peer sends.
        let _ = reader.read_u8()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for ParameterType {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ParameterType_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for ParameterType {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ParameterType";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: ::nros_serdes::schema::EMPTY_STRUCT_MEMBER,
        ty: ::nros_serdes::FieldType::Uint8,
        offset: 0,
    }];
}
