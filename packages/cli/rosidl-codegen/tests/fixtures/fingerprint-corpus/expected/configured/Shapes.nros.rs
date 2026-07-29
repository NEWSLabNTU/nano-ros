// emit:ok
// nros message type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Message: Shapes

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Shapes message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shapes {
    pub flag: bool,
    pub i8_v: i8,
    pub u8_v: u8,
    pub i16_v: i16,
    pub u16_v: u16,
    pub i32_v: i32,
    pub u32_v: u32,
    pub i64_v: i64,
    pub u64_v: u64,
    pub f32_v: f32,
    pub f64_v: f64,
    pub text: heapless::String<256>,
    pub seq_prim: heapless::Vec<i64, 64>,
    pub seq_string: heapless::Vec<heapless::String<256>, 64>,
    pub arr_fixed: [f64; 3],
    pub seq_bounded: heapless::Vec<i32, 4>,
    pub str_bounded: heapless::String<8>,
}

impl Serialize for Shapes {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_bool(self.flag)?;
        writer.write_i8(self.i8_v)?;
        writer.write_u8(self.u8_v)?;
        writer.write_i16(self.i16_v)?;
        writer.write_u16(self.u16_v)?;
        writer.write_i32(self.i32_v)?;
        writer.write_u32(self.u32_v)?;
        writer.write_i64(self.i64_v)?;
        writer.write_u64(self.u64_v)?;
        writer.write_f32(self.f32_v)?;
        writer.write_f64(self.f64_v)?;
        writer.write_string(self.text.as_str())?;
        writer.write_u32(self.seq_prim.len() as u32)?;
        for item in &self.seq_prim {
            writer.write_i64(*item)?;
        }
        writer.write_u32(self.seq_string.len() as u32)?;
        for item in &self.seq_string {
            writer.write_string(item.as_str())?;
        }
        for item in &self.arr_fixed {
            writer.write_f64(*item)?;
        }
        writer.write_u32(self.seq_bounded.len() as u32)?;
        for item in &self.seq_bounded {
            writer.write_i32(*item)?;
        }
        writer.write_string(self.str_bounded.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Shapes {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            flag: reader.read_bool()?,
            i8_v: reader.read_i8()?,
            u8_v: reader.read_u8()?,
            i16_v: reader.read_i16()?,
            u16_v: reader.read_u16()?,
            i32_v: reader.read_i32()?,
            u32_v: reader.read_u32()?,
            i64_v: reader.read_i64()?,
            u64_v: reader.read_u64()?,
            f32_v: reader.read_f32()?,
            f64_v: reader.read_f64()?,
            text: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            seq_prim: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            seq_string: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem = heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            arr_fixed: {
                let mut arr: [f64; 3] = Default::default();
                for i in 0..3 {
                    arr[i] = reader.read_f64()?;
                }
                arr
            },
            seq_bounded: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i32()?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            str_bounded: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Shapes {
    const TYPE_NAME: &'static str = "fingerprint-corpus::msg::dds_::Shapes_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const FT_SEQ_PRIM_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int64;
#[allow(non_upper_case_globals)]
pub const FT_SEQ_STRING_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
#[allow(non_upper_case_globals)]
pub const FT_ARR_FIXED_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Float64;
#[allow(non_upper_case_globals)]
pub const FT_SEQ_BOUNDED_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int32;
impl ::nros_serdes::Message for Shapes {
    const TYPE_NAME: &'static str = "fingerprint-corpus/msg/Shapes";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "flag",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(Shapes, flag),
        },
        ::nros_serdes::Field {
            name: "i8_v",
            ty: ::nros_serdes::FieldType::Int8,
            offset: ::core::mem::offset_of!(Shapes, i8_v),
        },
        ::nros_serdes::Field {
            name: "u8_v",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(Shapes, u8_v),
        },
        ::nros_serdes::Field {
            name: "i16_v",
            ty: ::nros_serdes::FieldType::Int16,
            offset: ::core::mem::offset_of!(Shapes, i16_v),
        },
        ::nros_serdes::Field {
            name: "u16_v",
            ty: ::nros_serdes::FieldType::Uint16,
            offset: ::core::mem::offset_of!(Shapes, u16_v),
        },
        ::nros_serdes::Field {
            name: "i32_v",
            ty: ::nros_serdes::FieldType::Int32,
            offset: ::core::mem::offset_of!(Shapes, i32_v),
        },
        ::nros_serdes::Field {
            name: "u32_v",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(Shapes, u32_v),
        },
        ::nros_serdes::Field {
            name: "i64_v",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(Shapes, i64_v),
        },
        ::nros_serdes::Field {
            name: "u64_v",
            ty: ::nros_serdes::FieldType::Uint64,
            offset: ::core::mem::offset_of!(Shapes, u64_v),
        },
        ::nros_serdes::Field {
            name: "f32_v",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(Shapes, f32_v),
        },
        ::nros_serdes::Field {
            name: "f64_v",
            ty: ::nros_serdes::FieldType::Float64,
            offset: ::core::mem::offset_of!(Shapes, f64_v),
        },
        ::nros_serdes::Field {
            name: "text",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Shapes, text),
        },
        ::nros_serdes::Field {
            name: "seq_prim",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SEQ_PRIM_ELEM),
            offset: ::core::mem::offset_of!(Shapes, seq_prim),
        },
        ::nros_serdes::Field {
            name: "seq_string",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SEQ_STRING_ELEM),
            offset: ::core::mem::offset_of!(Shapes, seq_string),
        },
        ::nros_serdes::Field {
            name: "arr_fixed",
            ty: ::nros_serdes::FieldType::Array(3, &FT_ARR_FIXED_ELEM),
            offset: ::core::mem::offset_of!(Shapes, arr_fixed),
        },
        ::nros_serdes::Field {
            name: "seq_bounded",
            ty: ::nros_serdes::FieldType::BoundedSequence(4, &FT_SEQ_BOUNDED_ELEM),
            offset: ::core::mem::offset_of!(Shapes, seq_bounded),
        },
        ::nros_serdes::Field {
            name: "str_bounded",
            ty: ::nros_serdes::FieldType::BoundedString(8),
            offset: ::core::mem::offset_of!(Shapes, str_bounded),
        },
];
}