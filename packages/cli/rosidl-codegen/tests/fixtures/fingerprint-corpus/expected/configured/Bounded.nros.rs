// emit:ok
// nros message type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Message: Bounded

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Bounded message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bounded {
    pub flag: bool,
    pub wide: i64,
    pub narrow: u8,
    pub d: f64,
    pub label: heapless::String<8>,
    pub fixed: [i32; 4],
}

impl Serialize for Bounded {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_bool(self.flag)?;
        writer.write_i64(self.wide)?;
        writer.write_u8(self.narrow)?;
        writer.write_f64(self.d)?;
        writer.write_string(self.label.as_str())?;
        for item in &self.fixed {
            writer.write_i32(*item)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Bounded {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            flag: reader.read_bool()?,
            wide: reader.read_i64()?,
            narrow: reader.read_u8()?,
            d: reader.read_f64()?,
            label: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            fixed: {
                let mut arr: [i32; 4] = Default::default();
                for i in 0..4 {
                    arr[i] = reader.read_i32()?;
                }
                arr
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Bounded {
    const TYPE_NAME: &'static str = "fingerprint-corpus::msg::dds_::Bounded_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_FIXED_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int32;
impl ::nros_serdes::Message for Bounded {
    const TYPE_NAME: &'static str = "fingerprint-corpus/msg/Bounded";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "flag",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(Bounded, flag),
        },
        ::nros_serdes::Field {
            name: "wide",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(Bounded, wide),
        },
        ::nros_serdes::Field {
            name: "narrow",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(Bounded, narrow),
        },
        ::nros_serdes::Field {
            name: "d",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(Bounded, d),
        },
        ::nros_serdes::Field {
            name: "label",
            ty: ::nros_serdes::FieldType::BoundedString(8),
            offset: ::core::mem::offset_of!(Bounded, label),
        },
        ::nros_serdes::Field {
            name: "fixed",
            ty: ::nros_serdes::FieldType::Array(4, &FT_FIXED_ELEM),
            offset: ::core::mem::offset_of!(Bounded, fixed),
        },
];
}