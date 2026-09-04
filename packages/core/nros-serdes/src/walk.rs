//! phase-421 W5 — the schema-driven serialization strategy (RFC-0088 D7).
//!
//! A provider that declares `impl = "schema"` in its `nros-serdes.toml` writes
//! ONE implementation and gets every message, with no codegen plugin and no
//! generated code per type. This module is the machinery that makes that true:
//! the walk over [`crate::schema::Field`] lives here, once, and a provider
//! supplies only the primitive encode/decode operations of its own wire.
//!
//! # The signature RFC-0088 D7 sketched cannot be implemented
//!
//! D7 wrote:
//!
//! ```text
//! fn serialize(msg: *const u8, schema: &'static [Field], out: &mut [u8]) -> …
//! ```
//!
//! That takes the HOST STRUCT as a pointer and reads fields at `Field::offset`.
//! Measured against the schemas codegen actually emits, it stops at the first
//! variable-length field, and it does so for three independent reasons:
//!
//! * **A `String` field's host type is `heapless::String<N>` and the schema
//!   does not carry `N`.** `nros generate-rust` emits
//!   `FieldType::String` — the IDL type — for a member whose Rust storage is a
//!   fixed-capacity buffer. `BoundedString(n)` is the IDL bound, not the host
//!   capacity, and the two are different numbers.
//! * **`heapless::String` / `heapless::Vec` are `repr(Rust)`.** Their field
//!   order and padding are unspecified, so "the length is at offset 0" is not a
//!   fact this crate is allowed to assume. `Field::offset` is well defined —
//!   `offset_of!` works on a `repr(Rust)` struct — but it only gets you to the
//!   START of the container, and the container's own interior is opaque.
//! * **A nested type's SIZE is not in the schema.** [`crate::schema::NestedType`]
//!   carries `type_name` and `fields`; striding an `Array(N, Nested(..))` or a
//!   sequence of structs needs `size_of` of the element, and the largest
//!   `offset` in a `repr(Rust)` child does not determine it.
//!
//! Extending the schema to carry host layout would change what codegen must
//! emit for every committed generated message, which is a different change from
//! this one. So the pivot moves: the value access a schema-driven provider has
//! today is **the CDR byte stream**, which nano-ros already produces for every
//! message from generated code. `impl = "schema"` is therefore a TRANSCODER
//! strategy in v1 — CDR in, foreign wire out, and back — and that is precisely
//! why the walk belongs here rather than in each provider.
//!
//! The cost is the one D7 already accepted: schema-driven is slower than the
//! per-type serializer we emit for CDR, and `impl = "codegen"` is the answer
//! when someone hits the wall.
//!
//! # What the walk does not cover
//!
//! `FieldType::WString` / `FieldType::BoundedWString` reach
//! [`SchemaError::Unsupported`], because [`crate::cdr::CdrReader`] has no
//! wide-string primitive to read them WITH — there is no `read_wstring`, in
//! either direction. No message in `packages/interfaces/*` uses one, so this is
//! a hole in the CDR codec that the schema walk inherits rather than one it
//! introduces.

use crate::{
    cdr::{CdrReader, CdrWriter},
    error::{DeserError, SerError},
    schema::{Field, FieldType, Message},
};

/// What can go wrong in a schema-driven encode or decode.
///
/// Deliberately separate from [`SerError`] / [`DeserError`]: those describe the
/// CDR side of a transcode, and a provider's own wire has failure modes CDR
/// does not (a truncated foreign buffer, a length prefix that disagrees with
/// the schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaError {
    /// The CDR side failed while reading.
    CdrRead(DeserError),
    /// The CDR side failed while writing.
    CdrWrite(SerError),
    /// The foreign buffer is too small to hold the encoding.
    BufferTooSmall,
    /// The foreign buffer ended in the middle of a value.
    Truncated,
    /// The foreign buffer disagrees with the schema — a length prefix past a
    /// declared bound, a bool that is not 0 or 1, invalid UTF-8.
    Malformed,
    /// A schema shape this walk cannot express, with the reason.
    ///
    /// Never a silent skip: a field that cannot be walked ends the encode,
    /// because a partial message on the wire is worse than no message.
    Unsupported(&'static str),
}

impl From<DeserError> for SchemaError {
    fn from(e: DeserError) -> Self {
        SchemaError::CdrRead(e)
    }
}

impl From<SerError> for SchemaError {
    fn from(e: SerError) -> Self {
        SchemaError::CdrWrite(e)
    }
}

impl core::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemaError::CdrRead(e) => write!(f, "cdr read: {e}"),
            SchemaError::CdrWrite(e) => write!(f, "cdr write: {e}"),
            SchemaError::BufferTooSmall => write!(f, "output buffer too small"),
            SchemaError::Truncated => write!(f, "input ended mid-value"),
            SchemaError::Malformed => write!(f, "input disagrees with the schema"),
            SchemaError::Unsupported(why) => write!(f, "unsupported schema shape: {why}"),
        }
    }
}

/// The encode half of a schema-driven format: the walk pushes typed values in,
/// the implementor writes its wire.
///
/// Every structural hook has a no-op default, so a flat format like the
/// reference `packed` provider implements only the primitives. A self-describing
/// format (one that writes field names, or a per-struct length) overrides the
/// hooks it needs.
pub trait SchemaSink {
    /// Entering a struct — the top-level message, or a nested member.
    fn struct_begin(&mut self, type_name: &str) -> Result<(), SchemaError> {
        let _ = type_name;
        Ok(())
    }
    /// Leaving the struct opened by the matching [`Self::struct_begin`].
    fn struct_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }
    /// Entering one field. Carries the whole [`Field`], so a format that writes
    /// names or types has them without a second lookup.
    fn field_begin(&mut self, field: &'static Field) -> Result<(), SchemaError> {
        let _ = field;
        Ok(())
    }
    /// Leaving the field opened by the matching [`Self::field_begin`].
    fn field_end(&mut self, field: &'static Field) -> Result<(), SchemaError> {
        let _ = field;
        Ok(())
    }
    /// A fixed-size array of `len` elements is about to be written. No length
    /// is on the wire unless the format chooses to put one there — `len` comes
    /// from the schema on both sides.
    fn array_begin(&mut self, len: usize) -> Result<(), SchemaError> {
        let _ = len;
        Ok(())
    }
    /// Leaving the array opened by the matching [`Self::array_begin`].
    fn array_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }
    /// A sequence of `len` elements is about to be written. Unlike an array
    /// this length is DATA, so a format must record it.
    fn seq_begin(&mut self, len: usize) -> Result<(), SchemaError>;
    /// Leaving the sequence opened by the matching [`Self::seq_begin`].
    fn seq_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }

    /// Write an IDL `boolean`.
    fn put_bool(&mut self, v: bool) -> Result<(), SchemaError>;
    /// Write an IDL `octet` / `uint8`.
    fn put_u8(&mut self, v: u8) -> Result<(), SchemaError>;
    /// Write an IDL `int8`.
    fn put_i8(&mut self, v: i8) -> Result<(), SchemaError>;
    /// Write an IDL `uint16`.
    fn put_u16(&mut self, v: u16) -> Result<(), SchemaError>;
    /// Write an IDL `int16`.
    fn put_i16(&mut self, v: i16) -> Result<(), SchemaError>;
    /// Write an IDL `uint32`.
    fn put_u32(&mut self, v: u32) -> Result<(), SchemaError>;
    /// Write an IDL `int32`.
    fn put_i32(&mut self, v: i32) -> Result<(), SchemaError>;
    /// Write an IDL `uint64`.
    fn put_u64(&mut self, v: u64) -> Result<(), SchemaError>;
    /// Write an IDL `int64`.
    fn put_i64(&mut self, v: i64) -> Result<(), SchemaError>;
    /// Write an IDL `float`.
    fn put_f32(&mut self, v: f32) -> Result<(), SchemaError>;
    /// Write an IDL `double`.
    fn put_f64(&mut self, v: f64) -> Result<(), SchemaError>;
    /// Write an IDL narrow `string`. The NUL CDR appends is a CDR concern and
    /// is already stripped — `v` is the payload.
    fn put_str(&mut self, v: &str) -> Result<(), SchemaError>;
}

/// The decode half: the walk pulls typed values out, in schema order.
///
/// Exactly dual to [`SchemaSink`]. The walk knows what comes next from the
/// schema, so the implementor never has to parse a type tag it did not write.
pub trait SchemaSource {
    /// Entering a struct — the top-level message, or a nested member.
    fn struct_begin(&mut self, type_name: &str) -> Result<(), SchemaError> {
        let _ = type_name;
        Ok(())
    }
    /// Leaving the struct opened by the matching [`Self::struct_begin`].
    fn struct_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }
    /// Entering one field.
    fn field_begin(&mut self, field: &'static Field) -> Result<(), SchemaError> {
        let _ = field;
        Ok(())
    }
    /// Leaving the field opened by the matching [`Self::field_begin`].
    fn field_end(&mut self, field: &'static Field) -> Result<(), SchemaError> {
        let _ = field;
        Ok(())
    }
    /// A fixed-size array of `len` elements follows; `len` is from the schema.
    fn array_begin(&mut self, len: usize) -> Result<(), SchemaError> {
        let _ = len;
        Ok(())
    }
    /// Leaving the array opened by the matching [`Self::array_begin`].
    fn array_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }
    /// Read the element count of a sequence off the wire.
    fn seq_begin(&mut self) -> Result<usize, SchemaError>;
    /// Leaving the sequence opened by the matching [`Self::seq_begin`].
    fn seq_end(&mut self) -> Result<(), SchemaError> {
        Ok(())
    }

    /// Read an IDL `boolean`.
    fn take_bool(&mut self) -> Result<bool, SchemaError>;
    /// Read an IDL `octet` / `uint8`.
    fn take_u8(&mut self) -> Result<u8, SchemaError>;
    /// Read an IDL `int8`.
    fn take_i8(&mut self) -> Result<i8, SchemaError>;
    /// Read an IDL `uint16`.
    fn take_u16(&mut self) -> Result<u16, SchemaError>;
    /// Read an IDL `int16`.
    fn take_i16(&mut self) -> Result<i16, SchemaError>;
    /// Read an IDL `uint32`.
    fn take_u32(&mut self) -> Result<u32, SchemaError>;
    /// Read an IDL `int32`.
    fn take_i32(&mut self) -> Result<i32, SchemaError>;
    /// Read an IDL `uint64`.
    fn take_u64(&mut self) -> Result<u64, SchemaError>;
    /// Read an IDL `int64`.
    fn take_i64(&mut self) -> Result<i64, SchemaError>;
    /// Read an IDL `float`.
    fn take_f32(&mut self) -> Result<f32, SchemaError>;
    /// Read an IDL `double`.
    fn take_f64(&mut self) -> Result<f64, SchemaError>;
    /// Read an IDL narrow `string`, borrowed out of the source buffer.
    fn take_str(&mut self) -> Result<&str, SchemaError>;
}

/// Walk `schema`, reading CDR and pushing each value into `sink`.
///
/// `reader` must be positioned at the start of the struct's members (past the
/// encapsulation header). The DHEADER handling mirrors a generated `serialize`
/// exactly: one per struct, top-level AND nested, a no-op under XCDR1 — the
/// same rule `crate::size` was written against.
pub fn encode_from_cdr<S: SchemaSink + ?Sized>(
    reader: &mut CdrReader<'_>,
    type_name: &str,
    schema: &'static [Field],
    sink: &mut S,
) -> Result<(), SchemaError> {
    let scope = reader.begin_dheader()?;
    sink.struct_begin(type_name)?;
    for field in schema {
        sink.field_begin(field)?;
        encode_one(reader, &field.ty, sink)?;
        sink.field_end(field)?;
    }
    sink.struct_end()?;
    reader.end_dheader(scope)?;
    Ok(())
}

fn encode_one<S: SchemaSink + ?Sized>(
    r: &mut CdrReader<'_>,
    ty: &'static FieldType,
    sink: &mut S,
) -> Result<(), SchemaError> {
    match ty {
        FieldType::Bool => sink.put_bool(r.read_bool()?),
        FieldType::Uint8 => sink.put_u8(r.read_u8()?),
        FieldType::Int8 => sink.put_i8(r.read_i8()?),
        FieldType::Uint16 => sink.put_u16(r.read_u16()?),
        FieldType::Int16 => sink.put_i16(r.read_i16()?),
        FieldType::Uint32 => sink.put_u32(r.read_u32()?),
        FieldType::Int32 => sink.put_i32(r.read_i32()?),
        FieldType::Uint64 => sink.put_u64(r.read_u64()?),
        FieldType::Int64 => sink.put_i64(r.read_i64()?),
        FieldType::Float32 => sink.put_f32(r.read_f32()?),
        FieldType::Float64 => sink.put_f64(r.read_f64()?),
        FieldType::String => sink.put_str(r.read_string()?),
        FieldType::BoundedString(n) => {
            let s = r.read_string()?;
            // The bound is an IDL fact the peer may have violated; refusing
            // here is what stops it becoming the provider's problem.
            if s.len() > *n {
                return Err(SchemaError::Malformed);
            }
            sink.put_str(s)
        }
        FieldType::WString | FieldType::BoundedWString(_) => Err(SchemaError::Unsupported(
            "wstring: CdrReader has no wide-string primitive to transcode from",
        )),
        FieldType::Nested(nested) => encode_from_cdr(r, nested.type_name, nested.fields, sink),
        FieldType::Array(n, inner) => {
            sink.array_begin(*n)?;
            for _ in 0..*n {
                encode_one(r, inner, sink)?;
            }
            sink.array_end()
        }
        FieldType::Sequence(inner) => {
            let n = r.read_sequence_len()?;
            sink.seq_begin(n)?;
            for _ in 0..n {
                encode_one(r, inner, sink)?;
            }
            sink.seq_end()
        }
        FieldType::BoundedSequence(cap, inner) => {
            let n = r.read_sequence_len()?;
            if n > *cap {
                return Err(SchemaError::Malformed);
            }
            sink.seq_begin(n)?;
            for _ in 0..n {
                encode_one(r, inner, sink)?;
            }
            sink.seq_end()
        }
    }
}

/// Walk `schema`, pulling each value from `source` and writing CDR.
///
/// The exact inverse of [`encode_from_cdr`], including the per-struct DHEADER,
/// so a value that survives one survives the pair byte-for-byte.
pub fn decode_to_cdr<S: SchemaSource + ?Sized>(
    source: &mut S,
    type_name: &str,
    schema: &'static [Field],
    writer: &mut CdrWriter<'_>,
) -> Result<(), SchemaError> {
    let mark = writer.begin_dheader()?;
    source.struct_begin(type_name)?;
    for field in schema {
        source.field_begin(field)?;
        decode_one(source, &field.ty, writer)?;
        source.field_end(field)?;
    }
    source.struct_end()?;
    writer.end_dheader(mark)?;
    Ok(())
}

fn decode_one<S: SchemaSource + ?Sized>(
    source: &mut S,
    ty: &'static FieldType,
    w: &mut CdrWriter<'_>,
) -> Result<(), SchemaError> {
    match ty {
        FieldType::Bool => w.write_bool(source.take_bool()?)?,
        FieldType::Uint8 => w.write_u8(source.take_u8()?)?,
        FieldType::Int8 => w.write_i8(source.take_i8()?)?,
        FieldType::Uint16 => w.write_u16(source.take_u16()?)?,
        FieldType::Int16 => w.write_i16(source.take_i16()?)?,
        FieldType::Uint32 => w.write_u32(source.take_u32()?)?,
        FieldType::Int32 => w.write_i32(source.take_i32()?)?,
        FieldType::Uint64 => w.write_u64(source.take_u64()?)?,
        FieldType::Int64 => w.write_i64(source.take_i64()?)?,
        FieldType::Float32 => w.write_f32(source.take_f32()?)?,
        FieldType::Float64 => w.write_f64(source.take_f64()?)?,
        FieldType::String => {
            let s = source.take_str()?;
            w.write_string(s)?;
        }
        FieldType::BoundedString(n) => {
            let s = source.take_str()?;
            if s.len() > *n {
                return Err(SchemaError::Malformed);
            }
            w.write_string(s)?;
        }
        FieldType::WString | FieldType::BoundedWString(_) => {
            return Err(SchemaError::Unsupported(
                "wstring: CdrWriter has no wide-string primitive to transcode into",
            ));
        }
        FieldType::Nested(nested) => {
            decode_to_cdr(source, nested.type_name, nested.fields, w)?;
        }
        FieldType::Array(n, inner) => {
            source.array_begin(*n)?;
            for _ in 0..*n {
                decode_one(source, inner, w)?;
            }
            source.array_end()?;
        }
        FieldType::Sequence(inner) => {
            let n = source.seq_begin()?;
            w.write_sequence_len(n)?;
            for _ in 0..n {
                decode_one(source, inner, w)?;
            }
            source.seq_end()?;
        }
        FieldType::BoundedSequence(cap, inner) => {
            let n = source.seq_begin()?;
            if n > *cap {
                return Err(SchemaError::Malformed);
            }
            w.write_sequence_len(n)?;
            for _ in 0..n {
                decode_one(source, inner, w)?;
            }
            source.seq_end()?;
        }
    }
    Ok(())
}

/// A serialization format implemented once, by walking the schema (RFC-0088 D7).
///
/// # How this differs from the D7 sketch, and why
///
/// D7 wrote `serialize(msg: *const u8, schema, out)`. The host struct pointer
/// is not usable — see the module docs for the three measured reasons — so the
/// message side of both methods is the CDR byte stream instead: a
/// [`CdrReader`] positioned at the members on the way out, a [`CdrWriter`] on
/// the way back. Everything else survives: one implementation, every message,
/// no generated code, the schema as the only description.
///
/// The second added parameter is `type_name`. A `&'static [Field]` slice has no
/// name of its own — [`Message::TYPE_NAME`] is a separate const, and nested
/// members carry theirs in [`crate::schema::NestedType`] — so without it the
/// top-level struct would be the one node a self-describing format could not
/// name. Use [`SchemaSerializer::serialize_message`] to supply both from a type.
pub trait SchemaSerializer {
    /// Cross-image identity (RFC-0088 D2). The string, never the number, is
    /// what crosses an image boundary.
    const FORMAT_NAME: &'static str;

    /// Image-local discriminant, as a raw `u8` rather than a
    /// [`crate::format::SerializationFormatId`]: the enum reserves values for
    /// in-tree formats, and a third-party provider is assigned one by the build
    /// from the set of formats its image declares. A provider that becomes
    /// in-tree gains a variant; one that does not still has a number here.
    const FORMAT_ID: u8;

    /// Encode one message from its CDR form into `out`, returning bytes written.
    fn serialize(
        msg: &mut CdrReader<'_>,
        type_name: &str,
        schema: &'static [Field],
        out: &mut [u8],
    ) -> Result<usize, SchemaError>;

    /// Decode one message from `bytes` into CDR, returning bytes consumed.
    fn deserialize(
        bytes: &[u8],
        type_name: &str,
        schema: &'static [Field],
        msg: &mut CdrWriter<'_>,
    ) -> Result<usize, SchemaError>;

    /// [`Self::serialize`] with the name and schema taken from the type.
    fn serialize_message<M: Message>(
        msg: &mut CdrReader<'_>,
        out: &mut [u8],
    ) -> Result<usize, SchemaError>
    where
        Self: Sized,
    {
        Self::serialize(msg, M::TYPE_NAME, M::FIELDS, out)
    }

    /// [`Self::deserialize`] with the name and schema taken from the type.
    fn deserialize_message<M: Message>(
        bytes: &[u8],
        msg: &mut CdrWriter<'_>,
    ) -> Result<usize, SchemaError>
    where
        Self: Sized,
    {
        Self::deserialize(bytes, M::TYPE_NAME, M::FIELDS, msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::NestedType;

    /// A sink that records the walk as text, so the ORDER of the callbacks is
    /// asserted rather than assumed. It writes no wire at all — the point is
    /// that a provider sees the structure, not just a flat value stream.
    #[derive(Default)]
    struct Trace {
        out: heapless::String<512>,
    }

    impl Trace {
        fn push(&mut self, s: &str) -> Result<(), SchemaError> {
            self.out
                .push_str(s)
                .map_err(|_| SchemaError::BufferTooSmall)
        }
    }

    impl SchemaSink for Trace {
        fn struct_begin(&mut self, type_name: &str) -> Result<(), SchemaError> {
            self.push("{")?;
            self.push(type_name)
        }
        fn struct_end(&mut self) -> Result<(), SchemaError> {
            self.push("}")
        }
        fn field_begin(&mut self, field: &'static Field) -> Result<(), SchemaError> {
            self.push(" ")?;
            self.push(field.name)?;
            self.push("=")
        }
        fn seq_begin(&mut self, len: usize) -> Result<(), SchemaError> {
            self.push(if len == 0 { "[0" } else { "[n" })
        }
        fn seq_end(&mut self) -> Result<(), SchemaError> {
            self.push("]")
        }
        fn put_bool(&mut self, _: bool) -> Result<(), SchemaError> {
            self.push("b")
        }
        fn put_u8(&mut self, _: u8) -> Result<(), SchemaError> {
            self.push("u8")
        }
        fn put_i8(&mut self, _: i8) -> Result<(), SchemaError> {
            self.push("i8")
        }
        fn put_u16(&mut self, _: u16) -> Result<(), SchemaError> {
            self.push("u16")
        }
        fn put_i16(&mut self, _: i16) -> Result<(), SchemaError> {
            self.push("i16")
        }
        fn put_u32(&mut self, _: u32) -> Result<(), SchemaError> {
            self.push("u32")
        }
        fn put_i32(&mut self, _: i32) -> Result<(), SchemaError> {
            self.push("i32")
        }
        fn put_u64(&mut self, _: u64) -> Result<(), SchemaError> {
            self.push("u64")
        }
        fn put_i64(&mut self, _: i64) -> Result<(), SchemaError> {
            self.push("i64")
        }
        fn put_f32(&mut self, _: f32) -> Result<(), SchemaError> {
            self.push("f32")
        }
        fn put_f64(&mut self, _: f64) -> Result<(), SchemaError> {
            self.push("f64")
        }
        fn put_str(&mut self, v: &str) -> Result<(), SchemaError> {
            self.push("\"")?;
            self.push(v)?;
            self.push("\"")
        }
    }

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
    const HEADER_FIELDS: &[Field] = &[
        Field {
            name: "stamp",
            ty: FieldType::Nested(&TIME),
            offset: 0,
        },
        Field {
            name: "frame_id",
            ty: FieldType::String,
            offset: 8,
        },
    ];

    fn header_cdr(buf: &mut [u8]) -> usize {
        let mut w = CdrWriter::new(buf);
        let dh = w.begin_dheader().unwrap();
        let inner = w.begin_dheader().unwrap();
        w.write_i32(7).unwrap();
        w.write_u32(8).unwrap();
        w.end_dheader(inner).unwrap();
        w.write_string("map").unwrap();
        w.end_dheader(dh).unwrap();
        w.position()
    }

    #[test]
    fn the_walk_visits_nested_structs_in_declaration_order() {
        let mut buf = [0u8; 64];
        let len = header_cdr(&mut buf);
        let mut r = CdrReader::new(&buf[..len]);
        let mut trace = Trace::default();
        encode_from_cdr(&mut r, "std_msgs/msg/Header", HEADER_FIELDS, &mut trace).unwrap();
        assert_eq!(
            trace.out.as_str(),
            "{std_msgs/msg/Header stamp={builtin_interfaces/msg/Time sec=i32 nanosec=u32} \
             frame_id=\"map\"}"
        );
    }

    #[test]
    fn a_wstring_is_refused_by_name_rather_than_skipped() {
        const WIDE: &[Field] = &[Field {
            name: "text",
            ty: FieldType::WString,
            offset: 0,
        }];
        let buf = [0u8; 16];
        let mut r = CdrReader::new(&buf);
        let mut trace = Trace::default();
        let err = encode_from_cdr(&mut r, "t/msg/W", WIDE, &mut trace).unwrap_err();
        match err {
            SchemaError::Unsupported(why) => assert!(why.contains("wstring"), "got {why:?}"),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_bounded_sequence_past_its_bound_is_malformed_not_accepted() {
        const ELEM: FieldType = FieldType::Uint8;
        const FIELDS: &[Field] = &[Field {
            name: "data",
            ty: FieldType::BoundedSequence(2, &ELEM),
            offset: 0,
        }];
        let mut buf = [0u8; 32];
        let len = {
            let mut w = CdrWriter::new(&mut buf);
            let dh = w.begin_dheader().unwrap();
            w.write_sequence_len(5).unwrap();
            for _ in 0..5 {
                w.write_u8(1).unwrap();
            }
            w.end_dheader(dh).unwrap();
            w.position()
        };
        let mut r = CdrReader::new(&buf[..len]);
        let mut trace = Trace::default();
        assert_eq!(
            encode_from_cdr(&mut r, "t/msg/B", FIELDS, &mut trace),
            Err(SchemaError::Malformed)
        );
    }
}
