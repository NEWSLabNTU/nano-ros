// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: ListParametersResult

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ListParametersResult message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersResult {
    pub names: heapless::Vec<heapless::String<256>, 64>,
    pub prefixes: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for ListParametersResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        writer.write_u32(self.prefixes.len() as u32)?;
        for item in &self.prefixes {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ListParametersResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            names: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem =
                        heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            prefixes: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem =
                        heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ListParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::ListParametersResult_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_NAMES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
#[allow(non_upper_case_globals)]
pub const FT_PREFIXES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for ListParametersResult {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/ListParametersResult";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "names",
            ty: ::nros_serdes::FieldType::Sequence(&FT_NAMES_ELEM),
            offset: ::core::mem::offset_of!(ListParametersResult, names),
        },
        ::nros_serdes::Field {
            name: "prefixes",
            ty: ::nros_serdes::FieldType::Sequence(&FT_PREFIXES_ELEM),
            offset: ::core::mem::offset_of!(ListParametersResult, prefixes),
        },
    ];
}
