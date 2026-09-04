//! `packed` — the reference schema-driven serialization provider (RFC-0088 D7,
//! phase-421 W5).
//!
//! # Why this format, and what it is not
//!
//! `packed` is a **test vehicle**. It makes no interop claim, nothing else
//! speaks it, and no backend selects it. It exists because the `impl =
//! "schema"` strategy needed a subject, and the two formats already in the tree
//! cannot be one:
//!
//! * **uORB** is the PX4 struct verbatim (RFC-0011). A "serializer" for it
//!   copies bytes and walks no schema at all, so it would prove nothing about
//!   the walk.
//! * **CDR** is the `impl = "codegen"` case — the strategy schema-driven
//!   serialization is the ALTERNATIVE to. Transcoding CDR to CDR would let a
//!   walk that mishandles alignment still pass.
//!
//! So the wire is the simplest thing that still differs from CDR in every
//! structural way a walk could get wrong. Each difference is a defect the
//! round-trip would surface:
//!
//! | `packed` | CDR | what a bug here would look like |
//! | --- | --- | --- |
//! | no alignment padding | pad to natural alignment (capped at 4 under XCDR2) | field values shift after the first odd-width member |
//! | `u32` byte length, no NUL | `u32` of `len + 1`, then the NUL | strings gain or lose a trailing byte per field |
//! | nothing per struct | a DHEADER per struct under XCDR2 | a nested struct swallows its parent's next field |
//! | `u32` count then elements | same | — (the one place they agree) |
//!
//! # Wire format
//!
//! Little-endian throughout, byte-packed, no padding anywhere.
//!
//! ```text
//! bool          1 byte, 0 or 1
//! u8 / i8       1 byte
//! u16 / i16     2 bytes LE
//! u32 / i32     4 bytes LE
//! u64 / i64     8 bytes LE
//! f32 / f64     4 / 8 bytes, IEEE-754 LE bit pattern
//! string        u32 LE byte count, then that many UTF-8 bytes. No NUL.
//! array<T, N>   N encodings of T, back to back. No count: N is in the schema.
//! sequence<T>   u32 LE element count, then that many encodings of T.
//! struct        its fields in declaration order. Nothing around them.
//! ```
//!
//! There is no self-description: the schema on both sides says what comes next.
//! That is the whole economy of `impl = "schema"` — the description already
//! exists in `.rodata`, so the wire does not have to carry it.
//!
//! # What it does not encode
//!
//! `wstring` / `bounded wstring`. Not a limitation of this format — the CDR
//! codec has no wide-string primitive in either direction, so there is nothing
//! to transcode from. The walk refuses those by name
//! (`SchemaError::Unsupported`); no message in `packages/interfaces/*` has one.

#![no_std]

use nros_serdes::{
    cdr::{CdrReader, CdrWriter},
    schema::Field,
    walk::{
        SchemaError, SchemaSerializer, SchemaSink, SchemaSource, decode_to_cdr, encode_from_cdr,
    },
};

/// Cross-image identity (RFC-0088 D2). Must equal the `name` in `package.xml`.
pub const FORMAT_NAME: &str = "packed";

/// Image-local discriminant. Must equal `format_id` in `nros-serdes.toml`.
///
/// A raw `u8`, not a `SerializationFormatId` variant: the enum reserves values
/// for formats a BACKEND speaks, and none speaks this one.
pub const FORMAT_ID: u8 = 3;

/// The `packed` format, as a type.
pub struct Packed;

/// Encoder: appends `packed` values into a caller-owned buffer.
pub struct PackedSink<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> PackedSink<'a> {
    /// A sink writing into `out` from byte 0.
    pub fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }

    /// Bytes written so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    fn raw(&mut self, bytes: &[u8]) -> Result<(), SchemaError> {
        let end = self
            .pos
            .checked_add(bytes.len())
            .ok_or(SchemaError::BufferTooSmall)?;
        if end > self.out.len() {
            return Err(SchemaError::BufferTooSmall);
        }
        self.out[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
        Ok(())
    }

    fn len_prefix(&mut self, n: usize) -> Result<(), SchemaError> {
        let n = u32::try_from(n).map_err(|_| SchemaError::Malformed)?;
        self.raw(&n.to_le_bytes())
    }
}

impl SchemaSink for PackedSink<'_> {
    fn seq_begin(&mut self, len: usize) -> Result<(), SchemaError> {
        self.len_prefix(len)
    }
    fn put_bool(&mut self, v: bool) -> Result<(), SchemaError> {
        self.raw(&[u8::from(v)])
    }
    fn put_u8(&mut self, v: u8) -> Result<(), SchemaError> {
        self.raw(&[v])
    }
    fn put_i8(&mut self, v: i8) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_u16(&mut self, v: u16) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_i16(&mut self, v: i16) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_u32(&mut self, v: u32) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_i32(&mut self, v: i32) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_u64(&mut self, v: u64) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_i64(&mut self, v: i64) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_f32(&mut self, v: f32) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_f64(&mut self, v: f64) -> Result<(), SchemaError> {
        self.raw(&v.to_le_bytes())
    }
    fn put_str(&mut self, v: &str) -> Result<(), SchemaError> {
        self.len_prefix(v.len())?;
        self.raw(v.as_bytes())
    }
}

/// Decoder: reads `packed` values out of a borrowed buffer.
pub struct PackedSource<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> PackedSource<'a> {
    /// A source reading `bytes` from byte 0.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Bytes consumed so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    fn raw(&mut self, n: usize) -> Result<&'a [u8], SchemaError> {
        let end = self.pos.checked_add(n).ok_or(SchemaError::Truncated)?;
        if end > self.bytes.len() {
            return Err(SchemaError::Truncated);
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], SchemaError> {
        let s = self.raw(N)?;
        let mut buf = [0u8; N];
        buf.copy_from_slice(s);
        Ok(buf)
    }

    fn len_prefix(&mut self) -> Result<usize, SchemaError> {
        Ok(u32::from_le_bytes(self.fixed::<4>()?) as usize)
    }
}

impl SchemaSource for PackedSource<'_> {
    fn seq_begin(&mut self) -> Result<usize, SchemaError> {
        let n = self.len_prefix()?;
        // A count larger than the bytes that remain cannot be honoured, and
        // believing it would mean a `for 0..n` that fails one element at a
        // time. Each element is at least one byte, so this is a sound floor.
        if n > self.bytes.len() - self.pos {
            return Err(SchemaError::Truncated);
        }
        Ok(n)
    }
    fn take_bool(&mut self) -> Result<bool, SchemaError> {
        match self.fixed::<1>()?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SchemaError::Malformed),
        }
    }
    fn take_u8(&mut self) -> Result<u8, SchemaError> {
        Ok(self.fixed::<1>()?[0])
    }
    fn take_i8(&mut self) -> Result<i8, SchemaError> {
        Ok(i8::from_le_bytes(self.fixed::<1>()?))
    }
    fn take_u16(&mut self) -> Result<u16, SchemaError> {
        Ok(u16::from_le_bytes(self.fixed::<2>()?))
    }
    fn take_i16(&mut self) -> Result<i16, SchemaError> {
        Ok(i16::from_le_bytes(self.fixed::<2>()?))
    }
    fn take_u32(&mut self) -> Result<u32, SchemaError> {
        Ok(u32::from_le_bytes(self.fixed::<4>()?))
    }
    fn take_i32(&mut self) -> Result<i32, SchemaError> {
        Ok(i32::from_le_bytes(self.fixed::<4>()?))
    }
    fn take_u64(&mut self) -> Result<u64, SchemaError> {
        Ok(u64::from_le_bytes(self.fixed::<8>()?))
    }
    fn take_i64(&mut self) -> Result<i64, SchemaError> {
        Ok(i64::from_le_bytes(self.fixed::<8>()?))
    }
    fn take_f32(&mut self) -> Result<f32, SchemaError> {
        Ok(f32::from_le_bytes(self.fixed::<4>()?))
    }
    fn take_f64(&mut self) -> Result<f64, SchemaError> {
        Ok(f64::from_le_bytes(self.fixed::<8>()?))
    }
    fn take_str(&mut self) -> Result<&str, SchemaError> {
        let n = self.len_prefix()?;
        let bytes = self.raw(n)?;
        core::str::from_utf8(bytes).map_err(|_| SchemaError::Malformed)
    }
}

impl SchemaSerializer for Packed {
    const FORMAT_NAME: &'static str = FORMAT_NAME;
    const FORMAT_ID: u8 = FORMAT_ID;

    fn serialize(
        msg: &mut CdrReader<'_>,
        type_name: &str,
        schema: &'static [Field],
        out: &mut [u8],
    ) -> Result<usize, SchemaError> {
        let mut sink = PackedSink::new(out);
        encode_from_cdr(msg, type_name, schema, &mut sink)?;
        Ok(sink.position())
    }

    fn deserialize(
        bytes: &[u8],
        type_name: &str,
        schema: &'static [Field],
        msg: &mut CdrWriter<'_>,
    ) -> Result<usize, SchemaError> {
        let mut source = PackedSource::new(bytes);
        decode_to_cdr(&mut source, type_name, schema, msg)?;
        Ok(source.position())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_serdes::schema::{FieldType, NestedType};

    const TIME_FIELDS: &[Field] = &[
        Field {
            name: "sec",
            ty: FieldType::Int32,
            offset: 0,
        },
        Field {
            name: "nanosec",
            ty: FieldType::Uint32,
            offset: 4,
        },
    ];
    const TIME: NestedType = NestedType {
        type_name: "builtin_interfaces/msg/Time",
        fields: TIME_FIELDS,
    };

    /// `u8` then `i64`: CDR pads seven bytes to reach 8-byte alignment,
    /// `packed` does not. If the walk ever inherited CDR's padding rule this
    /// assertion is the one that fails.
    #[test]
    fn packed_has_no_alignment_padding_where_cdr_does() {
        const FIELDS: &[Field] = &[
            Field {
                name: "tag",
                ty: FieldType::Uint8,
                offset: 0,
            },
            Field {
                name: "value",
                ty: FieldType::Int64,
                offset: 8,
            },
        ];

        let mut cdr = [0u8; 64];
        let cdr_len = {
            let mut w = CdrWriter::new(&mut cdr);
            w.write_u8(0xAA).unwrap();
            w.write_i64(-1).unwrap();
            w.position()
        };
        assert_eq!(cdr_len, 16, "1 byte + 7 pad + 8");

        let mut out = [0u8; 64];
        let mut r = CdrReader::new(&cdr[..cdr_len]);
        let n = Packed::serialize(&mut r, "t/msg/M", FIELDS, &mut out).unwrap();
        assert_eq!(n, 9, "packed writes 1 + 8 with nothing between");
        assert_eq!(
            &out[..9],
            &[0xAA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        );
    }

    /// A string is `len` bytes with no NUL, where CDR writes `len + 1` and one.
    #[test]
    fn a_string_carries_no_nul_and_no_off_by_one() {
        const FIELDS: &[Field] = &[Field {
            name: "text",
            ty: FieldType::String,
            offset: 0,
        }];

        let mut cdr = [0u8; 64];
        let cdr_len = {
            let mut w = CdrWriter::new(&mut cdr);
            w.write_string("hi").unwrap();
            w.position()
        };
        assert_eq!(cdr_len, 4 + 3, "CDR: u32 len+1, 'hi', NUL");

        let mut out = [0u8; 64];
        let mut r = CdrReader::new(&cdr[..cdr_len]);
        let n = Packed::serialize(&mut r, "t/msg/S", FIELDS, &mut out).unwrap();
        assert_eq!(&out[..n], &[2, 0, 0, 0, b'h', b'i']);
    }

    /// Nested structs are transparent: no DHEADER, no delimiter, nothing.
    #[test]
    fn a_nested_struct_adds_nothing_to_the_wire() {
        const FIELDS: &[Field] = &[Field {
            name: "stamp",
            ty: FieldType::Nested(&TIME),
            offset: 0,
        }];

        let mut cdr = [0u8; 64];
        let cdr_len = {
            // XCDR2, so the CDR side genuinely carries two DHEADERs the packed
            // side must drop.
            let mut w = CdrWriter::new_with_header_xcdr2(&mut cdr).unwrap();
            let outer = w.begin_dheader().unwrap();
            let inner = w.begin_dheader().unwrap();
            w.write_i32(1).unwrap();
            w.write_u32(2).unwrap();
            w.end_dheader(inner).unwrap();
            w.end_dheader(outer).unwrap();
            w.position()
        };

        let mut out = [0u8; 64];
        let mut r = CdrReader::new_with_header(&cdr[..cdr_len]).unwrap();
        let n = Packed::serialize(&mut r, "t/msg/N", FIELDS, &mut out).unwrap();
        assert_eq!(&out[..n], &[1, 0, 0, 0, 2, 0, 0, 0], "8 bytes, no headers");
    }

    #[test]
    fn a_short_output_buffer_errs_rather_than_truncating() {
        const FIELDS: &[Field] = &[Field {
            name: "value",
            ty: FieldType::Uint64,
            offset: 0,
        }];
        let mut cdr = [0u8; 16];
        let cdr_len = {
            let mut w = CdrWriter::new(&mut cdr);
            w.write_u64(7).unwrap();
            w.position()
        };
        let mut out = [0u8; 4];
        let mut r = CdrReader::new(&cdr[..cdr_len]);
        assert_eq!(
            Packed::serialize(&mut r, "t/msg/U", FIELDS, &mut out),
            Err(SchemaError::BufferTooSmall)
        );
    }

    #[test]
    fn a_truncated_input_errs_rather_than_reading_zero() {
        const FIELDS: &[Field] = &[Field {
            name: "value",
            ty: FieldType::Uint64,
            offset: 0,
        }];
        let mut cdr = [0u8; 32];
        let mut w = CdrWriter::new(&mut cdr);
        assert_eq!(
            Packed::deserialize(&[1, 2, 3], "t/msg/U", FIELDS, &mut w),
            Err(SchemaError::Truncated)
        );
    }

    #[test]
    fn the_declared_identity_matches_the_descriptor() {
        // The three spellings of one fact: this crate's consts, the trait
        // impl, and `nros-serdes.toml`. The first two are checked here; the
        // third by `check-serdes-descriptors` reading the announcement.
        assert_eq!(<Packed as SchemaSerializer>::FORMAT_NAME, "packed");
        assert_eq!(<Packed as SchemaSerializer>::FORMAT_ID, 3);
    }
}
