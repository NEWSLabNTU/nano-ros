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

use core::ops::ControlFlow;

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

/// How deep a field path [`first_unbounded`] will name before it stops.
///
/// ROS message nesting is shallow in practice (`PoseStamped.header.stamp.sec`
/// is three), and this walk runs on `no_std` with no allocator, so the path is
/// a fixed array rather than a `Vec`. A deeper type still reports — the path is
/// simply truncated, and [`UnboundedField::truncated`] says so, because a path
/// that silently stops short would point at the wrong member.
pub const MAX_FIELD_PATH_DEPTH: usize = 8;

/// Which unbounded member kind was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnboundedKind {
    /// `string` with no IDL bound.
    String,
    /// `wstring` with no IDL bound.
    WString,
    /// `sequence<T>` with no IDL bound.
    Sequence,
}

impl UnboundedKind {
    /// The IDL spelling, for a diagnostic.
    pub const fn as_str(self) -> &'static str {
        match self {
            UnboundedKind::String => "string",
            UnboundedKind::WString => "wstring",
            UnboundedKind::Sequence => "sequence<T>",
        }
    }
}

/// The first member that makes a type unbounded, named.
///
/// [`max_serialized_size`] answers `None`, which is honest and useless on its
/// own: a user told their type has no bound cannot act without knowing WHICH
/// member costs it, and the offender is routinely three structs down in a type
/// they did not write. This is that answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnboundedField {
    /// Field names from the root outward; only `depth` entries are meaningful.
    pub path: [&'static str; MAX_FIELD_PATH_DEPTH],
    /// How many entries of `path` are set.
    pub depth: usize,
    /// True when nesting exceeded [`MAX_FIELD_PATH_DEPTH`] and `path` names a
    /// prefix rather than the whole route.
    pub truncated: bool,
    /// What kind of member it is.
    pub kind: UnboundedKind,
}

impl core::fmt::Display for UnboundedField {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, seg) in self.path.iter().take(self.depth).enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            f.write_str(seg)?;
        }
        if self.truncated {
            f.write_str(".…")?;
        }
        write!(f, " ({})", self.kind.as_str())
    }
}

/// Report EVERY member that makes `fields` unbounded, in declaration order.
///
/// [`first_unbounded`] answers "which member costs the bound" one member at a
/// time, and that is the wrong shape for the caller that actually asks.
/// phase-403 W0 made an unbounded type a BUILD ERROR, and a stock ROS type is
/// routinely unbounded in several places at once — `nav_msgs/Odometry` has
/// `header.frame_id` and `child_frame_id`, and `sensor_msgs/PointCloud2` has
/// four. Naming only the first turns "bound your types" into cap, rebuild,
/// discover the next one, repeat, once per member, with a whole codegen run
/// between each step. One build should name everything that needs a bound.
///
/// THE walk. [`first_unbounded`] is expressed on top of this rather than beside
/// it: two walks of one schema is the shape the sizes-header mirror defect keeps
/// taking (issues 0088 -> 0268), and "the first thing this reports" is exactly
/// what "the first unbounded member" means, so there is nothing left for a
/// second implementation to say.
///
/// `visit` returns [`ControlFlow`] so a caller that wants only the first can
/// stop the walk rather than let it enumerate a whole type and discard all but
/// one answer. The return value is `Break` iff the visitor broke.
///
/// `&mut dyn FnMut` rather than `impl FnMut`: the walk recurses, and a recursive
/// generic function cannot name the closure type it would instantiate itself
/// with. `no_std` and allocation-free either way — nothing is collected here,
/// which is what lets an all-members form exist at all on a target with no
/// allocator. The COLLECTING is the caller's, and only codegen (`std`) does it.
pub fn visit_unbounded(
    fields: &'static [Field],
    visit: &mut dyn FnMut(UnboundedField) -> ControlFlow<()>,
) -> ControlFlow<()> {
    fn walk(
        fields: &'static [Field],
        prefix: &mut [&'static str; MAX_FIELD_PATH_DEPTH],
        depth: usize,
        visit: &mut dyn FnMut(UnboundedField) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        for field in fields {
            let truncated = depth >= MAX_FIELD_PATH_DEPTH;
            if !truncated {
                prefix[depth] = field.name;
            }
            let here = |kind: UnboundedKind| UnboundedField {
                path: *prefix,
                depth: if truncated {
                    MAX_FIELD_PATH_DEPTH
                } else {
                    depth + 1
                },
                truncated,
                kind,
            };
            match &field.ty {
                FieldType::String => visit(here(UnboundedKind::String))?,
                FieldType::WString => visit(here(UnboundedKind::WString))?,
                FieldType::Sequence(_) => visit(here(UnboundedKind::Sequence))?,
                // A fixed array or a bounded sequence is bounded only if its
                // ELEMENT is, and the element can itself be an unbounded
                // string — `string[4]` has no bound. The element carries no
                // name of its own, so it reports at the field's own path, and
                // ONCE: several offenders inside one unnamed element would all
                // print the same path, which reads as a repeated line rather
                // than as more information.
                FieldType::Array(_, inner) | FieldType::BoundedSequence(_, inner) => {
                    if let Some(kind) = element_unbounded(inner) {
                        visit(here(kind))?;
                    }
                }
                FieldType::Nested(nested) => {
                    if truncated {
                        // Cannot record another segment; report the deepest
                        // path we can name rather than descending silently.
                        walk(nested.fields, prefix, depth, &mut |mut u| {
                            u.truncated = true;
                            visit(u)
                        })?;
                    } else {
                        walk(nested.fields, prefix, depth + 1, visit)?;
                    }
                }
                _ => {}
            }
        }
        ControlFlow::Continue(())
    }

    /// An element type is not a field, so it has no name — report only its kind.
    fn element_unbounded(ty: &'static FieldType) -> Option<UnboundedKind> {
        match ty {
            FieldType::String => Some(UnboundedKind::String),
            FieldType::WString => Some(UnboundedKind::WString),
            FieldType::Sequence(_) => Some(UnboundedKind::Sequence),
            FieldType::Array(_, inner) | FieldType::BoundedSequence(_, inner) => {
                element_unbounded(inner)
            }
            FieldType::Nested(nested) => {
                // A nested struct inside an array: any unbounded member counts.
                first_unbounded(nested.fields).map(|u| u.kind)
            }
            _ => None,
        }
    }

    let mut prefix = [""; MAX_FIELD_PATH_DEPTH];
    walk(fields, &mut prefix, 0, visit)
}

/// Find the first member that makes `fields` unbounded, if any.
///
/// Returns `None` exactly when [`size_bound`] reports `bounded` — the two walk
/// the same schema by the same rules, so a type that has a bound has no
/// offender to name and vice versa. That agreement is asserted by test, not
/// assumed: two walks of one schema is the shape the sizes-header mirror defect
/// keeps taking (issues 0088 -> 0268), so the second one exists only because it
/// answers a question the first cannot (WHICH member) and must be checked
/// against it.
///
/// The walk is [`visit_unbounded`]; this is its "stop at the first one" caller.
/// Reach for [`visit_unbounded`] when the diagnostic should name every member a
/// user has to fix, which is what codegen wants now that an unbounded type is a
/// build error (phase-403 W0).
///
/// Not `const`: it recurses through `&'static NestedType`, and the array
/// bookkeeping is not worth expressing in a const walk when every caller is a
/// diagnostic path.
pub fn first_unbounded(fields: &'static [Field]) -> Option<UnboundedField> {
    let mut found = None;
    let _ = visit_unbounded(fields, &mut |u| {
        found = Some(u);
        ControlFlow::Break(())
    });
    found
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

/// Phase 392 W3a — the type's own receive-buffer bound, or `None` when it has
/// none.
///
/// The VALUE behind [`bound_fits`]'s predicate. A subscription that knows its
/// type's bound can be routed to a size class instead of forcing the GLOBAL
/// buffer knob up: `ZPICO_SUBSCRIBER_BUFFER_SIZE` multiplies across
/// `MAX_SUBSCRIBERS x RING_DEPTH`, so raising it from 1024 to 4096 for one
/// 4 KiB topic costs 98,304 bytes, while the large class that topic belongs in
/// is already reserved and empty.
///
/// Takes the larger of the two encodings, for the same reason `bound_fits`
/// does: the peer picks the encoding at runtime, so sizing from XCDR1 alone is
/// a trap.
///
/// `None` means "no bound EXISTS", never "unknown" — phase 380 is explicit that
/// a buffer must not be sized from a fallback. A caller routing by size class
/// must treat `None` as "keep the default", not as "assume small".
pub const fn max_serialized_bound<M: crate::schema::Message>() -> Option<usize> {
    match (M::MAX_SERIALIZED_SIZE_XCDR1, M::MAX_SERIALIZED_SIZE_XCDR2) {
        (Some(x1), Some(x2)) => Some(if x1 > x2 { x1 } else { x2 }),
        // One encoding unbounded makes the type unbounded on the wire, because
        // the peer chooses. Not `Some(the_other)`.
        _ => None,
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

    // ========================================================================
    // issue 0896 layer 0 — naming the member that costs the bound
    // ========================================================================

    /// The two walks must agree. `first_unbounded` is a SECOND walk of the same
    /// schema, which is the shape the sizes-header mirror defect keeps taking
    /// (0088 -> 0268), so it earns its existence only by answering a question
    /// `size_bound` cannot — and only while it never disagrees about the
    /// question they share.
    fn agree(fields: &'static [Field]) {
        for version in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let bounded = size_bound(fields, version, 0).bounded;
            let named = first_unbounded(fields);
            assert_eq!(
                bounded,
                named.is_none(),
                "size_bound says bounded={bounded} but first_unbounded says \
                 {named:?} ({version:?})"
            );
        }
    }

    #[test]
    fn a_bounded_type_has_no_offender_to_name() {
        static FIELDS: &[Field] = &[
            f("x", FieldType::Uint32),
            f("s", FieldType::BoundedString(8)),
        ];
        agree(FIELDS);
    }

    #[test]
    fn an_unbounded_string_is_named() {
        static FIELDS: &[Field] = &[f("flag", FieldType::Bool), f("label", FieldType::String)];
        agree(FIELDS);
        let u = first_unbounded(FIELDS).unwrap();
        assert_eq!(u.kind, UnboundedKind::String);
        assert_eq!(&u.path[..u.depth], &["label"]);
    }

    /// The offender is usually not at the top level — this is the whole reason
    /// the path exists rather than a bare field name.
    #[test]
    fn a_nested_offender_reports_its_full_path() {
        static INNER: &[Field] = &[f("sec", FieldType::Int32), f("frame_id", FieldType::String)];
        static INNER_TY: NestedType = NestedType {
            type_name: "std_msgs/msg/Header",
            fields: INNER,
        };
        static FIELDS: &[Field] = &[f("header", FieldType::Nested(&INNER_TY))];
        agree(FIELDS);
        let u = first_unbounded(FIELDS).unwrap();
        assert_eq!(&u.path[..u.depth], &["header", "frame_id"]);
    }

    /// `string[4]` is a FIXED array of an UNBOUNDED element: the count is
    /// known and the bound is not. Reported at the field's own path, because
    /// an element has no name of its own.
    #[test]
    fn a_fixed_array_of_unbounded_elements_is_unbounded() {
        static ELEM: FieldType = FieldType::String;
        static FIELDS: &[Field] = &[f("names", FieldType::Array(4, &ELEM))];
        agree(FIELDS);
        let u = first_unbounded(FIELDS).unwrap();
        assert_eq!(&u.path[..u.depth], &["names"]);
        assert_eq!(u.kind, UnboundedKind::String);
    }

    /// A bounded sequence of bounded elements IS bounded — the easy way to get
    /// this wrong is to treat any sequence as unbounded.
    #[test]
    fn a_bounded_sequence_of_bounded_elements_is_bounded() {
        static ELEM: FieldType = FieldType::Uint16;
        static FIELDS: &[Field] = &[f("ranges", FieldType::BoundedSequence(16, &ELEM))];
        agree(FIELDS);
        assert!(first_unbounded(FIELDS).is_none());
    }

    #[test]
    fn the_first_offender_wins_so_the_message_names_one_thing() {
        static FIELDS: &[Field] = &[
            f("a", FieldType::String),
            f("b", FieldType::Sequence(&FieldType::Uint8)),
        ];
        let u = first_unbounded(FIELDS).unwrap();
        assert_eq!(&u.path[..u.depth], &["a"]);
    }

    /// A stack-only `core::fmt::Write` sink.
    ///
    /// `alloc::format!` would be shorter and would test the wrong build:
    /// `Display` here exists FOR the `no_std` diagnostic path, so the test runs
    /// where that path runs. Silently drops overflow — the assertion below
    /// catches a truncated result either way.
    struct Buf {
        bytes: [u8; 128],
        len: usize,
    }

    impl core::fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for b in s.as_bytes() {
                if self.len < self.bytes.len() {
                    self.bytes[self.len] = *b;
                    self.len += 1;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn the_display_form_reads_as_a_path_and_a_kind() {
        use core::fmt::Write;
        static INNER: &[Field] = &[f("frame_id", FieldType::String)];
        static INNER_TY: NestedType = NestedType {
            type_name: "std_msgs/msg/Header",
            fields: INNER,
        };
        static FIELDS: &[Field] = &[f("header", FieldType::Nested(&INNER_TY))];
        let u = first_unbounded(FIELDS).unwrap();

        let mut buf = Buf {
            bytes: [0; 128],
            len: 0,
        };
        write!(buf, "{u}").unwrap();
        assert_eq!(
            core::str::from_utf8(&buf.bytes[..buf.len]).unwrap(),
            "header.frame_id (string)"
        );
    }

    // ========================================================================
    // phase-403 W0 — naming EVERY member, not the first
    // ========================================================================

    /// Collect a whole type's offenders. `heapless` rather than `Vec` because
    /// this crate is `no_std`; that is also the point of the visitor shape.
    fn all(fields: &'static [Field]) -> heapless::Vec<UnboundedField, 16> {
        let mut out = heapless::Vec::new();
        let _ = visit_unbounded(fields, &mut |u| {
            let _ = out.push(u);
            ControlFlow::Continue(())
        });
        out
    }

    fn names(fields: &'static [Field]) -> heapless::Vec<&'static str, 16> {
        let mut out = heapless::Vec::new();
        for u in all(fields) {
            let _ = out.push(u.path[u.depth - 1]);
        }
        out
    }

    /// An unbounded type is a build error now, and a stock ROS type is unbounded
    /// in several places at once, so one build has to name all of them —
    /// otherwise bounding a package is one cap and one full codegen run per
    /// member.
    #[test]
    fn every_unbounded_member_is_visited_in_declaration_order() {
        static FIELDS: &[Field] = &[
            f("a", FieldType::String),
            f("keep", FieldType::Int32),
            f("b", FieldType::Sequence(&FieldType::Int64)),
            f("c", FieldType::WString),
        ];
        assert_eq!(names(FIELDS).as_slice(), &["a", "b", "c"]);
    }

    /// Nested members are visited too, and a bounded sibling does not stop the
    /// walk from continuing past the struct that contained one.
    #[test]
    fn the_walk_continues_past_a_nested_offender_to_its_siblings() {
        static INNER: &[Field] = &[f("frame_id", FieldType::String)];
        static INNER_TY: NestedType = NestedType {
            type_name: "std_msgs/msg/Header",
            fields: INNER,
        };
        static FIELDS: &[Field] = &[
            f("header", FieldType::Nested(&INNER_TY)),
            f("child_frame_id", FieldType::String),
        ];
        assert_eq!(names(FIELDS).as_slice(), &["frame_id", "child_frame_id"]);
        // The path, not just the leaf name, so a diagnostic can be acted on.
        assert_eq!(all(FIELDS)[0].path[0], "header");
        assert_eq!(all(FIELDS)[0].depth, 2);
    }

    /// `first_unbounded` is now expressed on top of this walk, so it must still
    /// answer exactly what the walk reports first — and must still stop, rather
    /// than enumerating a type and discarding all but one answer.
    #[test]
    fn the_first_offender_is_the_first_one_visited() {
        static INNER: &[Field] = &[f("frame_id", FieldType::String)];
        static INNER_TY: NestedType = NestedType {
            type_name: "std_msgs/msg/Header",
            fields: INNER,
        };
        static FLAT: &[Field] = &[f("a", FieldType::String), f("b", FieldType::String)];
        static NESTED: &[Field] = &[
            f("header", FieldType::Nested(&INNER_TY)),
            f("tail", FieldType::String),
        ];
        static CLEAN: &[Field] = &[f("only", FieldType::Int32)];
        for fields in [FLAT, NESTED, CLEAN] {
            assert_eq!(first_unbounded(fields), all(fields).first().copied());
        }
    }

    /// A visitor that breaks stops the walk where it broke — which is the
    /// mechanism `first_unbounded` uses, asserted rather than assumed.
    #[test]
    fn a_visitor_that_breaks_stops_the_walk() {
        static FIELDS: &[Field] = &[
            f("a", FieldType::String),
            f("b", FieldType::String),
            f("c", FieldType::String),
        ];
        let mut seen = 0usize;
        let flow = visit_unbounded(FIELDS, &mut |_| {
            seen += 1;
            if seen == 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(seen, 2);
        assert_eq!(flow, ControlFlow::Break(()));
    }

    /// A bounded type has nothing to report, from either form.
    #[test]
    fn a_bounded_type_visits_nothing() {
        static FIELDS: &[Field] = &[
            f("a", FieldType::BoundedString(8)),
            f("b", FieldType::Int32),
        ];
        assert!(all(FIELDS).is_empty());
        assert!(first_unbounded(FIELDS).is_none());
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

    /// Phase 392 W3a — `max_serialized_bound` is what routes a subscription to a
    /// size class, so the two ways it can be wrong both cost real RAM or real
    /// truncation: reporting the smaller encoding under-sizes the block the peer
    /// may actually fill, and inventing a number for an unbounded type sizes a
    /// buffer from a fallback, which phase 380 forbids in as many words.
    struct BoundedMsg;
    impl crate::schema::Message for BoundedMsg {
        const TYPE_NAME: &'static str = "test/msg/BoundedMsg";
        const FIELDS: &'static [Field] = &[
            f("a", FieldType::Uint8),
            f("b", FieldType::Uint64),
            f("c", FieldType::Uint8),
        ];
    }

    struct UnboundedMsg;
    impl crate::schema::Message for UnboundedMsg {
        const TYPE_NAME: &'static str = "test/msg/UnboundedMsg";
        const FIELDS: &'static [Field] = &[f("s", FieldType::String)];
    }

    #[test]
    fn max_serialized_bound_takes_the_larger_encoding() {
        let x1 = <BoundedMsg as crate::schema::Message>::MAX_SERIALIZED_SIZE_XCDR1
            .expect("bounded in xcdr1");
        let x2 = <BoundedMsg as crate::schema::Message>::MAX_SERIALIZED_SIZE_XCDR2
            .expect("bounded in xcdr2");
        let got = max_serialized_bound::<BoundedMsg>().expect("bounded type has a bound");
        assert_eq!(
            got,
            x1.max(x2),
            "must take the LARGER encoding — the peer picks it at runtime, so \
             sizing from one alone is the trap phase 380 documents"
        );
        assert!(
            bound_fits::<BoundedMsg>(got),
            "the value must satisfy the predicate it is derived from"
        );
        assert!(
            !bound_fits::<BoundedMsg>(got - 1),
            "one byte under the bound must NOT fit, or the value is not tight"
        );
    }

    #[test]
    fn max_serialized_bound_is_none_for_an_unbounded_type() {
        assert_eq!(
            max_serialized_bound::<UnboundedMsg>(),
            None,
            "an unbounded type has NO bound; returning a number here would let a \
             caller size a buffer from a fallback"
        );
        assert!(
            bound_fits::<UnboundedMsg>(1),
            "unbounded still passes the BUILD assertion — nothing is provable"
        );
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
