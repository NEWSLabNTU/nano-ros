//! Phase 380 W1 — a message's serialized size, computed instead of guessed.
//!
//! `NROS_SUBSCRIPTION_BUFFER_SIZE` defaults to 1024 bytes and is a GUESS.
//! Nothing checks it against the messages an image actually subscribes to, so a
//! sample that does not fit is dropped AFTER the transport ACKed it, and
//! `report_dropped_take` can only say "raise the knob" because the runtime does
//! not know what value would have worked. On a target that knob is static RAM
//! nobody can spare. Issue 0776 is the gap; this module is the calculator.
//!
//! # Why this is not a vtable slot
//!
//! Settled in phase-376 W4: nothing about a size bound varies by backend, and
//! upstream proves it by not varying at all — both `librmw_cyclonedds_cpp.so`
//! and `librmw_fastrtps_cpp.so` answer `rmw_get_serialized_message_size` with
//! `RMW_RET_UNSUPPORTED`, and nothing in a Humble install calls it. Upstream can
//! afford that because its serialized buffer RESIZES; the bound is a hint that
//! saves a realloc. Ours cannot resize, so the same number is load-bearing here.
//!
//! # Thread the offset — do not sum the maxima
//!
//! Padding is a function of WHERE a field starts, so a calculation that sums
//! per-field maxima is wrong the moment a variable-length field shifts what
//! follows. [`size_bound`] therefore takes `current_alignment` in and returns
//! the size from there, which is also what makes nested structs compose with no
//! special case — the same signature rosidl's generated Fast-RTPS support uses
//! (`max_serialized_size_T(full_bounded, is_plain, current_alignment)`).
//!
//! # Agreement with the writer is the only thing that matters
//!
//! Every rule below was read out of `CdrWriter`, not out of the CDR spec:
//! `align()` caps alignment at 4 under XCDR2 and honours 8 under XCDR1;
//! `begin_dheader()` aligns to 4 and reserves 4 bytes for EVERY struct under
//! XCDR2 (a generated `serialize` opens with it, so nested structs get one too);
//! `write_string` writes `len + 1` and then the NUL. A bound that merely looks
//! right is worthless — see `size_tests.rs`, which checks it against the bytes
//! the writer actually produced.

use crate::{
    cdr::EncodingVersion,
    schema::{Field, FieldType},
};

/// What one walk of a schema can say about size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeBound {
    /// Bytes, given the starting offset the walk was handed.
    pub bytes: usize,
    /// False once an unbounded `String` / `WString` / `Sequence` is reached —
    /// `bytes` is then a FLOOR, not a bound, and callers must not size a buffer
    /// from it.
    pub bounded: bool,
    /// No variable-length member anywhere, so the layout is fixed: `bytes` is
    /// EXACT rather than an upper bound, and the type is loan-eligible
    /// (phase-380 W5 wires this to `borrow_loaned_message` /
    /// `subscription_supports_in_place` rather than letting a second notion of
    /// "fixed layout" grow).
    pub plain: bool,
}

/// The encapsulation header the payload crossing our vtable carries: 2 bytes of
/// representation id + 2 of options.
///
/// TOP LEVEL ONLY, not per nested struct — which is why the Cyclone backend
/// computes `total = paylen + 4`. [`size_bound`] deliberately does not include
/// it; [`max_serialized_size`] does.
pub const ENCAPSULATION_HEADER_BYTES: usize = 4;

/// Padding needed to reach `alignment` from `offset`, under `version`.
///
/// Mirrors `CdrWriter::align` exactly, including the XCDR2 cap at 4 — a message
/// containing an `int64` therefore has TWO different bounds, and a single
/// constant would be silently wrong for one encoding.
const fn pad_to(offset: usize, alignment: usize, version: EncodingVersion) -> usize {
    let alignment = match version {
        EncodingVersion::Xcdr2 => {
            if alignment > 4 {
                4
            } else {
                alignment
            }
        }
        EncodingVersion::Xcdr1 => alignment,
    };
    if alignment == 0 {
        return 0;
    }
    (alignment - (offset % alignment)) % alignment
}

/// Size of one field, starting at `offset`. Returns the new offset plus the two
/// flags, folded by the caller.
const fn field_bound(
    ty: &FieldType,
    version: EncodingVersion,
    offset: usize,
) -> (usize, bool, bool) {
    match ty {
        FieldType::Bool | FieldType::Uint8 | FieldType::Int8 => (offset + 1, true, true),
        FieldType::Uint16 | FieldType::Int16 => {
            let o = offset + pad_to(offset, 2, version);
            (o + 2, true, true)
        }
        FieldType::Uint32 | FieldType::Int32 | FieldType::Float32 => {
            let o = offset + pad_to(offset, 4, version);
            (o + 4, true, true)
        }
        FieldType::Uint64 | FieldType::Int64 | FieldType::Float64 => {
            let o = offset + pad_to(offset, 8, version);
            (o + 8, true, true)
        }
        // `write_string` writes `len + 1` as the u32 prefix and then the NUL,
        // so a bound of `n` payload bytes costs `4 + n + 1`.
        FieldType::BoundedString(n) => {
            let o = offset + pad_to(offset, 4, version);
            (o + 4 + *n + 1, true, false)
        }
        FieldType::BoundedWString(n) => {
            let o = offset + pad_to(offset, 4, version);
            (o + 4 + 2 * *n, true, false)
        }
        // No bound exists. Account the length prefix so `bytes` is an honest
        // floor, and clear `bounded` so nobody sizes a buffer from it.
        FieldType::String | FieldType::WString | FieldType::Sequence(_) => {
            let o = offset + pad_to(offset, 4, version);
            (o + 4, false, false)
        }
        FieldType::Array(n, inner) => {
            let mut o = offset;
            let mut bounded = true;
            let mut plain = true;
            let mut i = 0;
            while i < *n {
                let (next, b, p) = field_bound(inner, version, o);
                o = next;
                bounded &= b;
                plain &= p;
                i += 1;
            }
            // A fixed array of plain elements is itself plain; of anything else,
            // not — the element count is fixed but each element's size is not.
            (o, bounded, plain)
        }
        FieldType::BoundedSequence(n, inner) => {
            let mut o = offset + pad_to(offset, 4, version);
            o += 4;
            let mut bounded = true;
            let mut i = 0;
            while i < *n {
                let (next, b, _) = field_bound(inner, version, o);
                o = next;
                bounded &= b;
                i += 1;
            }
            // Never plain: the wire length varies with the actual element count
            // even though its maximum is known.
            (o, bounded, false)
        }
        FieldType::Nested(nested) => {
            let inner = size_bound(nested.fields, version, offset);
            (inner.bytes, inner.bounded, inner.plain)
        }
    }
}

/// Walk `fields` from `current_alignment`, returning the size bound of the
/// struct they describe.
///
/// The returned `bytes` is an ABSOLUTE offset — the position after the last
/// field, measured from the same origin `current_alignment` was measured from —
/// so a nested struct composes by being handed the parent's current offset.
/// Subtract the starting offset if a length is what you want.
///
/// Excludes the encapsulation header; see [`max_serialized_size`].
pub const fn size_bound(
    fields: &'static [Field],
    version: EncodingVersion,
    current_alignment: usize,
) -> SizeBound {
    let mut offset = current_alignment;
    let mut bounded = true;
    let mut plain = true;

    // XCDR2 delimits EVERY appendable struct, nested ones included: a generated
    // `serialize` opens with `begin_dheader()`, which aligns to 4 and reserves
    // 4. Under XCDR1 the call is a no-op. Missing this UNDER-reports, which is
    // the dangerous direction — an under-reported bound sizes a buffer too
    // small and reintroduces the very drop this exists to stop.
    if matches!(version, EncodingVersion::Xcdr2) {
        offset += pad_to(offset, 4, version);
        offset += 4;
        // A DHEADER does not make a struct non-plain: its width is fixed.
    }

    let mut i = 0;
    while i < fields.len() {
        let (next, b, p) = field_bound(&fields[i].ty, version, offset);
        offset = next;
        bounded &= b;
        plain &= p;
        i += 1;
    }

    SizeBound {
        bytes: offset,
        bounded,
        plain,
    }
}

/// The whole payload a publisher hands the transport: encapsulation header plus
/// the struct's body, starting from offset 0.
///
/// `None` when the type is unbounded — the honest answer, and the one that keeps
/// a caller from sizing a buffer off a floor. Reach for `serialized_size(&self)`
/// (phase-380 W3) when an unbounded type still needs a number for THIS message.
pub const fn max_serialized_size(
    fields: &'static [Field],
    version: EncodingVersion,
) -> Option<usize> {
    // The body's offsets are measured from after the encapsulation header —
    // that is where `CdrWriter`'s `origin` sits — so the walk starts at 0 and
    // the header is added once, at the top.
    let bound = size_bound(fields, version, 0);
    if bound.bounded {
        Some(ENCAPSULATION_HEADER_BYTES + bound.bytes)
    } else {
        None
    }
}

/// Phase 380 W3 — the EXACT serialized size of THIS message.
///
/// Two questions get asked about size and they are not the same one:
///
/// | question | asked by | this module |
/// | --- | --- | --- |
/// | how large can this TYPE ever be? | build-time buffer sizing | [`max_serialized_size`] |
/// | how large is THIS message? | a publisher before publishing; a drop report | `serialized_size` |
///
/// The second is the only honest answer for an unbounded type, where the first
/// is `None` — a `String` field has no maximum, but the string in hand has a
/// length. It is what lets a drop report name the number that would have worked
/// instead of saying "raise the knob".
///
/// Exact by construction: it runs the REAL writer with its stores disabled
/// (`CdrWriter::measuring`), so the count comes from the same code that emits
/// the bytes. A second walk of the schema could not see the actual string
/// lengths and sequence counts, and would be a second implementation to keep in
/// step besides.
///
/// Includes the encapsulation header, matching [`max_serialized_size`], so the
/// two are directly comparable — which is the whole point at a call site
/// deciding whether a message fits.
pub fn serialized_size<T: crate::traits::Serialize>(
    value: &T,
    version: EncodingVersion,
) -> Result<usize, crate::error::SerError> {
    let mut w = crate::cdr::CdrWriter::measuring(&mut [], version);
    value.serialize(&mut w)?;
    Ok(ENCAPSULATION_HEADER_BYTES + w.position())
}

/// Phase 380 W4 — does a receive buffer of `rx_buf` bytes fit every message of
/// this type?
///
/// `true` when the type is bounded and the bound fits. **`false` when the type
/// is UNBOUNDED**, deliberately: no finite buffer fits a `String`, so the
/// honest answer to "is this guaranteed to fit" is no. A caller that wants
/// "fits unless proven otherwise" is asking a different question and should
/// look at [`serialized_size`] per message.
///
/// Const, so a call site can put it in a `const { assert!(...) }` and turn a
/// runtime drop into a build error:
///
/// ```ignore
/// const { assert!(buffer_fits::<Odometry>(RX_BUF, EncodingVersion::Xcdr1)) };
/// ```
///
/// # Why this is not simply a bound on `Subscription`
///
/// `Subscription<M, RX_BUF>` bounds `M: RosMessage`, which is a DIFFERENT trait
/// from [`crate::schema::Message`] — and measured 2026-08-26, hand-written
/// `RosMessage` types with no schema DO exist (`nros-core/src/service.rs`, the
/// component-runtime tests). Tightening that bound would break them, so the
/// assertion is opt-in at sites where the schema is known rather than universal
/// at a site where it is not. See the phase doc for what closing that gap needs.
pub const fn buffer_fits<M: crate::schema::Message>(
    rx_buf: usize,
    version: EncodingVersion,
) -> bool {
    let bound = match version {
        EncodingVersion::Xcdr1 => M::MAX_SERIALIZED_SIZE_XCDR1,
        EncodingVersion::Xcdr2 => M::MAX_SERIALIZED_SIZE_XCDR2,
    };
    match bound {
        Some(n) => rx_buf >= n,
        None => false,
    }
}

/// Phase 380 W4 — the BUILD-ASSERTION predicate: `false` only when the type is
/// provably too large for `rx_buf`.
///
/// Distinct from [`buffer_fits`], and the difference is the whole reason both
/// exist:
///
/// * `buffer_fits` answers "is this GUARANTEED to fit", so an unbounded type is
///   `false` — no finite buffer fits a `String`.
/// * this answers "can we PROVE it will not fit", so an unbounded type is
///   `true` — there is nothing to check, and failing the build for every
///   `std_msgs/String` subscription would be absurd.
///
/// Asserting `buffer_fits` at a subscription site was my first attempt and
/// would have refused the most common message in ROS. Kept as two named
/// functions rather than one with a flag, because the wrong one is silently
/// plausible at either call site.
///
/// Checks BOTH encodings and takes the larger: the encoding is a runtime
/// property of the peer, so a buffer sized from XCDR1 alone is a trap — XCDR2
/// adds a DHEADER per struct and is the bigger of the two for nested types.
pub const fn bound_fits<M: crate::schema::Message>(rx_buf: usize) -> bool {
    let x1 = match M::MAX_SERIALIZED_SIZE_XCDR1 {
        Some(n) => n,
        None => return true, // unbounded: nothing to prove
    };
    let x2 = match M::MAX_SERIALIZED_SIZE_XCDR2 {
        Some(n) => n,
        None => return true,
    };
    let largest = if x1 > x2 { x1 } else { x2 };
    rx_buf >= largest
}

/// Phase 380 W5 — is this type loan-eligible?
///
/// A loan hands out a pointer into the transport's own memory, which is sound
/// only when the layout is fixed: no length prefix to chase, no variable
/// member. That is exactly [`SizeBound::plain`], so the answer falls out of W1
/// rather than becoming a second notion of "fixed layout" maintained by hand —
/// which is the drift `borrow_loaned_message` and
/// `subscription_supports_in_place` would otherwise each grow their own version
/// of.
pub const fn is_loan_eligible<M: crate::schema::Message>() -> bool {
    M::IS_PLAIN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cdr::CdrWriter,
        schema::{Field, NestedType},
    };

    const fn f(name: &'static str, ty: FieldType) -> Field {
        Field {
            name,
            ty,
            offset: 0,
        }
    }

    /// Serialize a MAXIMAL instance of a schema with the real `CdrWriter`, and
    /// return the byte length the writer produced (header included).
    ///
    /// This is the whole point of the test module: a bound that is merely
    /// self-consistent proves nothing. Every assertion below compares
    /// [`max_serialized_size`] against bytes this function actually wrote.
    fn write_maximal(fields: &[Field], version: EncodingVersion, buf: &mut [u8]) -> usize {
        let mut w = match version {
            EncodingVersion::Xcdr1 => CdrWriter::new_with_header(buf).unwrap(),
            EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(buf).unwrap(),
        };
        let dh = w.begin_dheader().unwrap();
        write_fields(&mut w, fields);
        w.end_dheader(dh).unwrap();
        w.position()
    }

    fn write_fields(w: &mut CdrWriter<'_>, fields: &[Field]) {
        for field in fields {
            write_one(w, &field.ty);
        }
    }

    fn write_one(w: &mut CdrWriter<'_>, ty: &FieldType) {
        match ty {
            FieldType::Bool => w.write_bool(true).unwrap(),
            FieldType::Uint8 => w.write_u8(0xAB).unwrap(),
            FieldType::Int8 => w.write_i8(-1).unwrap(),
            FieldType::Uint16 => w.write_u16(0xBEEF).unwrap(),
            FieldType::Int16 => w.write_i16(-2).unwrap(),
            FieldType::Uint32 => w.write_u32(0xDEADBEEF).unwrap(),
            FieldType::Int32 => w.write_i32(-3).unwrap(),
            FieldType::Float32 => w.write_f32(1.5).unwrap(),
            FieldType::Uint64 => w.write_u64(u64::MAX).unwrap(),
            FieldType::Int64 => w.write_i64(-4).unwrap(),
            FieldType::Float64 => w.write_f64(2.5).unwrap(),
            // Maximal = exactly `n` payload bytes, which is what the bound
            // claims room for.
            FieldType::BoundedString(n) => {
                let s = "x".repeat(*n);
                w.write_string(&s).unwrap()
            }
            FieldType::String => w.write_string("").unwrap(),
            FieldType::Array(n, inner) => {
                for _ in 0..*n {
                    write_one(w, inner);
                }
            }
            FieldType::BoundedSequence(n, inner) => {
                w.write_u32(*n as u32).unwrap();
                for _ in 0..*n {
                    write_one(w, inner);
                }
            }
            FieldType::Nested(nested) => {
                let dh = w.begin_dheader().unwrap();
                write_fields(w, nested.fields);
                w.end_dheader(dh).unwrap();
            }
            other => panic!("test writer has no maximal value for {other:?}"),
        }
    }

    /// The core property, in both encodings: the computed bound is an upper
    /// bound on what the writer produces, and an EXACT one when `plain`.
    fn assert_agrees(fields: &'static [Field], version: EncodingVersion) {
        let mut buf = [0u8; 4096];
        let actual = write_maximal(fields, version, &mut buf);
        let bound = max_serialized_size(fields, version).expect("these fixtures are all bounded");
        assert!(
            actual <= bound,
            "{version:?}: writer produced {actual} bytes, bound claimed {bound} — \
             an UNDER-reported bound sizes a buffer too small, which is the drop \
             this module exists to stop"
        );
        if size_bound(fields, version, 0).plain {
            assert_eq!(
                actual, bound,
                "{version:?}: a plain type's bound must be EXACT, not merely an \
                 upper bound"
            );
        }
    }

    fn both(fields: &'static [Field]) {
        assert_agrees(fields, EncodingVersion::Xcdr1);
        assert_agrees(fields, EncodingVersion::Xcdr2);
    }

    // `builtin_interfaces/Time` — the plain case, and the one whose bound must
    // be exact.
    static TIME: &[Field] = &[f("sec", FieldType::Int32), f("nanosec", FieldType::Uint32)];

    #[test]
    fn plain_struct_bound_is_exact() {
        both(TIME);
        assert!(size_bound(TIME, EncodingVersion::Xcdr1, 0).plain);
    }

    /// The defect issue 0776 calls out first: a message containing an `int64`
    /// has TWO different bounds, because `CdrWriter::align` honours 8 under
    /// XCDR1 and caps at 4 under XCDR2. A single constant is silently wrong for
    /// one of them.
    #[test]
    fn eight_byte_alignment_differs_by_encoding() {
        // Two `int64`s, deliberately: with only ONE the two encodings come out
        // EQUAL, and that is a coincidence rather than a missing cap. For
        // `[u8, i64]` XCDR1 pads 7 before the i64 while XCDR2 caps alignment at
        // 4 and pads 3 — saving exactly the 4 bytes its DHEADER costs, so both
        // totals are 20. A test built on that single field would have "passed"
        // while asserting nothing, and would keep passing if the cap were
        // deleted. Repeating the pattern breaks the tie: the padding saving
        // scales with the number of 8-byte members, the DHEADER does not.
        static S: &[Field] = &[
            f("flag", FieldType::Uint8),
            f("big", FieldType::Int64),
            f("flag2", FieldType::Uint8),
            f("big2", FieldType::Int64),
        ];
        both(S);
        let x1 = max_serialized_size(S, EncodingVersion::Xcdr1).unwrap();
        let x2 = max_serialized_size(S, EncodingVersion::Xcdr2).unwrap();
        assert_ne!(
            x1, x2,
            "8-byte primitives must pad differently under the two encodings; if \
             these agree the alignment cap is not being applied"
        );
        assert!(
            x1 > x2,
            "XCDR1 aligns 8-byte primitives to 8 and so must be the larger of \
             the two here ({x1} vs {x2})"
        );
    }

    /// The coincidence above, pinned so nobody "simplifies" the test back into
    /// it: for this one layout the encodings genuinely agree, and a bound that
    /// reported one number for both would look correct here and nowhere else.
    #[test]
    fn a_single_int64_makes_the_two_encodings_agree_by_coincidence() {
        static S: &[Field] = &[f("flag", FieldType::Uint8), f("big", FieldType::Int64)];
        both(S);
        assert_eq!(
            max_serialized_size(S, EncodingVersion::Xcdr1),
            max_serialized_size(S, EncodingVersion::Xcdr2),
            "XCDR2's DHEADER (+4) exactly offsets the padding its alignment cap \
             saves (-4) for this shape"
        );
    }

    /// Padding depends on WHERE a field starts, so summing per-field maxima is
    /// wrong. Same fields, different order, different size.
    #[test]
    fn offset_is_threaded_not_summed() {
        static A: &[Field] = &[f("a", FieldType::Uint8), f("b", FieldType::Uint32)];
        static B: &[Field] = &[f("b", FieldType::Uint32), f("a", FieldType::Uint8)];
        both(A);
        both(B);
        assert_ne!(
            max_serialized_size(A, EncodingVersion::Xcdr1),
            max_serialized_size(B, EncodingVersion::Xcdr1),
            "u8,u32 pads and u32,u8 does not — a sum of maxima cannot tell them apart"
        );
    }

    #[test]
    fn bounded_string_and_sequence_are_bounded_but_not_plain() {
        static S: &[Field] = &[
            f("name", FieldType::BoundedString(8)),
            f("vals", FieldType::BoundedSequence(3, &FieldType::Uint32)),
        ];
        both(S);
        let b = size_bound(S, EncodingVersion::Xcdr1, 0);
        assert!(b.bounded);
        assert!(!b.plain, "variable-length members are not loan-eligible");
    }

    #[test]
    fn nested_struct_composes_at_the_parent_offset() {
        static NESTED: NestedType = NestedType {
            type_name: "builtin_interfaces/msg/Time",
            fields: TIME,
        };
        static S: &[Field] = &[
            f("flag", FieldType::Uint8),
            f("stamp", FieldType::Nested(&NESTED)),
        ];
        both(S);
    }

    /// An unbounded member makes the type unbounded, and `max_serialized_size`
    /// must answer `None` rather than a floor someone could size a buffer from.
    #[test]
    fn unbounded_member_yields_none() {
        static S: &[Field] = &[
            f(
                "stamp",
                FieldType::Nested(&NestedType {
                    type_name: "builtin_interfaces/msg/Time",
                    fields: TIME,
                }),
            ),
            f("frame_id", FieldType::String),
        ];
        assert_eq!(max_serialized_size(S, EncodingVersion::Xcdr1), None);
        assert!(!size_bound(S, EncodingVersion::Xcdr1, 0).bounded);
        assert!(!size_bound(S, EncodingVersion::Xcdr1, 0).plain);
    }

    /// XCDR2 delimits every struct, nested ones included. Missing the nested
    /// DHEADER under-reports — the dangerous direction.
    #[test]
    fn xcdr2_counts_a_dheader_per_struct() {
        static INNER: NestedType = NestedType {
            type_name: "t/Inner",
            fields: TIME,
        };
        static FLAT: &[Field] = &[f("sec", FieldType::Int32), f("nanosec", FieldType::Uint32)];
        static WRAPPED: &[Field] = &[f("inner", FieldType::Nested(&INNER))];
        both(WRAPPED);
        let flat = max_serialized_size(FLAT, EncodingVersion::Xcdr2).unwrap();
        let wrapped = max_serialized_size(WRAPPED, EncodingVersion::Xcdr2).unwrap();
        assert_eq!(
            wrapped,
            flat + 4,
            "the nested struct's own DHEADER must be counted under XCDR2"
        );
        // ...and must NOT be under XCDR1, where begin_dheader is a no-op.
        assert_eq!(
            max_serialized_size(WRAPPED, EncodingVersion::Xcdr1).unwrap(),
            max_serialized_size(FLAT, EncodingVersion::Xcdr1).unwrap(),
            "XCDR1 has no DHEADER; wrapping must cost nothing"
        );
    }
}
