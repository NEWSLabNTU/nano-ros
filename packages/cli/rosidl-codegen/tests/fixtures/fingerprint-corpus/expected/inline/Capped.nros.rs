// emit:ok
// nros message type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Message: Capped

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Capped message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capped {
    pub label: heapless::String<256>,
    pub samples: heapless::Vec<i64, 64>,
    pub tags: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for Capped {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.label.as_str())?;
        writer.write_u32(self.samples.len() as u32)?;
        for item in &self.samples {
            writer.write_i64(*item)?;
        }
        writer.write_u32(self.tags.len() as u32)?;
        for item in &self.tags {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Capped {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            label: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            samples: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            tags: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem = heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Capped {
    const TYPE_NAME: &'static str = "fingerprint-corpus::msg::dds_::Capped_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_SAMPLES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int64;
#[allow(non_upper_case_globals)]
pub const FT_TAGS_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for Capped {
    const TYPE_NAME: &'static str = "fingerprint-corpus/msg/Capped";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "label",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Capped, label),
        },
        ::nros_serdes::Field {
            name: "samples",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SAMPLES_ELEM),
            offset: ::core::mem::offset_of!(Capped, samples),
        },
        ::nros_serdes::Field {
            name: "tags",
            ty: ::nros_serdes::FieldType::Sequence(&FT_TAGS_ELEM),
            offset: ::core::mem::offset_of!(Capped, tags),
        },
];
}