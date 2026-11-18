// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: TypeHash

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// TypeHash message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypeHash {
    pub version: u8,
    pub value: [u8; 32],
}

impl Serialize for TypeHash {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u8(self.version)?;
        for item in &self.value {
            writer.write_u8(*item)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for TypeHash {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            version: reader.read_u8()?,
            value: {
                let mut arr: [u8; 32] = Default::default();
                for i in 0..32 {
                    arr[i] = reader.read_u8()?;
                }
                arr
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for TypeHash {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::TypeHash_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // Issue 1179 -- the type's bound on `RosMessage`, the one trait every
    // backend's registration can read. Computed from the schema below, not
    // copied, so it cannot drift from it.
    const MAX_SERIALIZED_BOUND: Option<usize> = ::nros_serdes::size::max_serialized_bound::<Self>();
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_VALUE_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Uint8;
impl ::nros_serdes::Message for TypeHash {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/TypeHash";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "version",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(TypeHash, version),
        },
        ::nros_serdes::Field {
            name: "value",
            ty: ::nros_serdes::FieldType::Array(32, &FT_VALUE_ELEM),
            offset: ::core::mem::offset_of!(TypeHash, value),
        },
    ];
}
