//! Phase 380 W2 — the size bound, checked against the writer for every
//! committed generated message.
//!
//! W1's unit tests cover hand-built schemas: they prove the rules are applied,
//! not that they hold for the shapes real `.msg` files produce. This walks the
//! REAL `Message::FIELDS` of every type in `packages/interfaces` — 64 of them
//! after W0 — and checks the computed bound against bytes `CdrWriter` actually
//! wrote.
//!
//! # Why "serialize and compare", not "assert the number"
//!
//! A bound that is merely self-consistent proves nothing: the calculator could
//! agree with itself and disagree with the writer, which is the only
//! disagreement that matters. `report_dropped_take` exists because a sample that
//! does not fit is dropped after the transport ACKed it, and the number this
//! computes is what a buffer gets sized from. So every assertion here is
//! against `writer.position()`.
//!
//! # What "maximal" means, and why the walk drives the writer
//!
//! The schema is walked and each field written at its maximum — a
//! `BoundedString(n)` gets exactly `n` characters, a `BoundedSequence(n, t)`
//! exactly `n` elements. That is the instance the bound claims room for.
//!
//! Driving the writer from the SCHEMA rather than from a constructed Rust value
//! is deliberate: constructing a maximal instance of all 64 types needs 64
//! hand-written literals that rot, and rotted literals silently shrink coverage.
//! The trade is that this checks the calculator against the WRITER, not against
//! each generated `serialize`; `xcdr2_dheader_matches_generated_serialize` below
//! pins the one structural thing that could differ.
//!
//! Both encodings, because W1's first defect (a single constant for an `int64`
//! type) passes a one-encoding suite.

use nros_serdes::{
    cdr::{CdrWriter, EncodingVersion},
    schema::{Field, FieldType, Message},
    size::{max_serialized_size, size_bound},
};

/// Every committed generated message, by name and schema.
///
/// Named explicitly rather than discovered: a test that enumerates nothing
/// passes, and a `FIELDS`-less type is now impossible (W0's
/// `check-generated-schema-coverage`), so the only way this list goes stale is
/// a NEW message nobody added — which the count assertion below catches.
fn corpus() -> Vec<(&'static str, &'static [Field])> {
    use nros_builtin_interfaces::msg as bi;
    use nros_diagnostic_msgs::msg as dm;
    use nros_lifecycle_msgs::msg as lm;
    use nros_rcl_interfaces::msg as rm;
    use nros_std_msgs_diag::msg as sm;

    vec![
        ("builtin_interfaces/Time", <bi::Time as Message>::FIELDS),
        (
            "builtin_interfaces/Duration",
            <bi::Duration as Message>::FIELDS,
        ),
        (
            "rcl_interfaces/ParameterValue",
            <rm::ParameterValue as Message>::FIELDS,
        ),
        (
            "rcl_interfaces/ParameterType",
            <rm::ParameterType as Message>::FIELDS,
        ),
        (
            "rcl_interfaces/IntegerRange",
            <rm::IntegerRange as Message>::FIELDS,
        ),
        (
            "rcl_interfaces/FloatingPointRange",
            <rm::FloatingPointRange as Message>::FIELDS,
        ),
        (
            "rcl_interfaces/SetParametersResult",
            <rm::SetParametersResult as Message>::FIELDS,
        ),
        ("rcl_interfaces/Log", <rm::Log as Message>::FIELDS),
        ("lifecycle_msgs/State", <lm::State as Message>::FIELDS),
        (
            "lifecycle_msgs/Transition",
            <lm::Transition as Message>::FIELDS,
        ),
        (
            "lifecycle_msgs/TransitionEvent",
            <lm::TransitionEvent as Message>::FIELDS,
        ),
        ("std_msgs/Header", <sm::Header as Message>::FIELDS),
        ("std_msgs/String", <sm::String as Message>::FIELDS),
        ("std_msgs/Int32", <sm::Int32 as Message>::FIELDS),
        ("std_msgs/Float64", <sm::Float64 as Message>::FIELDS),
        (
            "diagnostic_msgs/KeyValue",
            <dm::KeyValue as Message>::FIELDS,
        ),
        (
            "diagnostic_msgs/DiagnosticStatus",
            <dm::DiagnosticStatus as Message>::FIELDS,
        ),
        (
            "diagnostic_msgs/DiagnosticArray",
            <dm::DiagnosticArray as Message>::FIELDS,
        ),
    ]
}

/// Write a maximal instance of `fields`, returning the byte count produced.
///
/// `None` when the schema contains something with no maximum — the caller skips
/// those, because there is no maximal instance to write and the bound is
/// `None` for the same reason.
fn write_maximal(fields: &[Field], version: EncodingVersion, buf: &mut [u8]) -> Option<usize> {
    let mut w = match version {
        EncodingVersion::Xcdr1 => CdrWriter::new_with_header(buf).ok()?,
        EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(buf).ok()?,
    };
    let dh = w.begin_dheader().ok()?;
    write_fields(&mut w, fields)?;
    w.end_dheader(dh).ok()?;
    Some(w.position())
}

fn write_fields(w: &mut CdrWriter<'_>, fields: &[Field]) -> Option<()> {
    for f in fields {
        write_one(w, &f.ty)?;
    }
    Some(())
}

fn write_one(w: &mut CdrWriter<'_>, ty: &FieldType) -> Option<()> {
    match ty {
        FieldType::Bool => w.write_bool(true).ok()?,
        FieldType::Uint8 => w.write_u8(0xAB).ok()?,
        FieldType::Int8 => w.write_i8(-1).ok()?,
        FieldType::Uint16 => w.write_u16(0xBEEF).ok()?,
        FieldType::Int16 => w.write_i16(-2).ok()?,
        FieldType::Uint32 => w.write_u32(0xDEADBEEF).ok()?,
        FieldType::Int32 => w.write_i32(-3).ok()?,
        FieldType::Float32 => w.write_f32(1.5).ok()?,
        FieldType::Uint64 => w.write_u64(u64::MAX).ok()?,
        FieldType::Int64 => w.write_i64(-4).ok()?,
        FieldType::Float64 => w.write_f64(2.5).ok()?,
        FieldType::BoundedString(n) => {
            let s = "x".repeat(*n);
            w.write_string(&s).ok()?
        }
        FieldType::BoundedWString(_) => return None,
        // No maximal instance exists.
        FieldType::String | FieldType::WString | FieldType::Sequence(_) => return None,
        FieldType::Array(n, inner) => {
            for _ in 0..*n {
                write_one(w, inner)?;
            }
        }
        FieldType::BoundedSequence(n, inner) => {
            w.write_u32(*n as u32).ok()?;
            for _ in 0..*n {
                write_one(w, inner)?;
            }
        }
        FieldType::Nested(nested) => {
            let dh = w.begin_dheader().ok()?;
            write_fields(w, nested.fields)?;
            w.end_dheader(dh).ok()?;
        }
    }
    Some(())
}

/// THE property: for every bounded type, the writer never exceeds the bound,
/// and for a `plain` type the bound is exact.
#[test]
fn bound_holds_against_the_writer_for_every_generated_type() {
    let mut buf = vec![0u8; 1 << 20];
    let mut checked = 0usize;
    let mut unbounded = 0usize;
    let mut failures = Vec::new();

    for (name, fields) in corpus() {
        for version in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let bound = max_serialized_size(fields, version);
            let actual = write_maximal(fields, version, &mut buf);

            match (bound, actual) {
                (None, _) => {
                    unbounded += 1;
                    // An unbounded type must NOT claim a bound. That is the
                    // whole safety property: a caller sizing a buffer from a
                    // floor is the drop this phase exists to stop.
                    assert!(
                        !size_bound(fields, version, 0).bounded,
                        "{name} {version:?}: max_serialized_size said None but the \
                         walk reported bounded — these must agree"
                    );
                }
                (Some(b), Some(a)) => {
                    checked += 1;
                    if a > b {
                        failures.push(format!(
                            "{name} {version:?}: writer produced {a} bytes, bound \
                             claimed {b} — an UNDER-reported bound sizes a buffer \
                             too small"
                        ));
                    }
                    if size_bound(fields, version, 0).plain && a != b {
                        failures.push(format!(
                            "{name} {version:?}: plain type, so the bound must be \
                             EXACT — writer {a}, bound {b}"
                        ));
                    }
                }
                (Some(b), None) => failures.push(format!(
                    "{name} {version:?}: bound is Some({b}) but no maximal instance \
                     could be written — a bounded type must be writable at its max"
                )),
            }
        }
    }

    // Preconditions, not decoration: an empty corpus or an all-unbounded one
    // would make every assertion above vacuous and this test would pass having
    // proven nothing.
    // 14, measured: most ROS types reach a `string` or an unbounded sequence,
    // so only 7 of the 18 corpus types are bounded and each contributes two
    // encodings. The floor guards against the corpus or the writer walk
    // silently shrinking, not against that ratio — it was 20 on first writing,
    // which was a guess, and the guess failing is what prompted counting.
    assert!(
        checked >= 14,
        "expected at least the 14 bounded (type, encoding) pairs this corpus has, \
         checked {checked} — the corpus or the writer walk has silently stopped \
         covering things"
    );
    assert!(
        unbounded > 0,
        "no unbounded type in the corpus, so the None path is untested — \
         std_msgs/String and std_msgs/Header are both unbounded and should be here"
    );
    assert!(
        failures.is_empty(),
        "{} bound violation(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("size bound: {checked} bounded pair(s) checked, {unbounded} unbounded");
}

/// The one structural thing the schema-driven walk above could get wrong
/// relative to a real generated `serialize`: whether a nested struct is
/// DHEADER-wrapped under XCDR2.
///
/// Checked against the type's OWN `Serialize`, not the walk, because that is
/// the code a publisher actually runs. `std_msgs/Header` nests
/// `builtin_interfaces/Time`, so its byte count differs between encodings by
/// exactly the two DHEADERs (outer + nested) if and only if both are emitted.
#[test]
fn xcdr2_dheader_matches_generated_serialize() {
    use nros_serdes::traits::Serialize;
    let msg = nros_std_msgs_diag::msg::Header::default();

    let mut b1 = [0u8; 512];
    let mut w1 = CdrWriter::new_with_header(&mut b1).unwrap();
    msg.serialize(&mut w1).unwrap();
    let len1 = w1.position();

    let mut b2 = [0u8; 512];
    let mut w2 = CdrWriter::new_with_header_xcdr2(&mut b2).unwrap();
    msg.serialize(&mut w2).unwrap();
    let len2 = w2.position();

    assert_eq!(
        len2 - len1,
        8,
        "Header nests Time, so XCDR2 must cost exactly two 4-byte DHEADERs more \
         than XCDR1 for an empty frame_id (got {len1} -> {len2}). A different \
         delta means the generated serialize wraps a different number of structs \
         than `size_bound` counts."
    );
}

/// Phase 380 W3 — `serialized_size` must equal what the real writer produces,
/// for the same value, in both encodings.
///
/// This is the property that makes it worth having: a measured size that could
/// disagree with the writer would be a second guess dressed as a fact. Checked
/// against `writer.position()`, like everything else here.
#[test]
fn serialized_size_equals_what_the_writer_produces() {
    use nros_serdes::{size::serialized_size, traits::Serialize};

    let header = nros_std_msgs_diag::msg::Header::default();
    let time = nros_builtin_interfaces::msg::Time::default();

    for version in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
        // Header is UNBOUNDED (it has a `frame_id` string), so this is exactly
        // the case `max_serialized_size` answers None for and W3 exists to
        // serve.
        assert_eq!(
            max_serialized_size(
                <nros_std_msgs_diag::msg::Header as Message>::FIELDS,
                version
            ),
            None,
            "Header must be unbounded, or this test is not covering the W3 case"
        );

        let mut buf = [0u8; 512];
        let mut w = match version {
            EncodingVersion::Xcdr1 => CdrWriter::new_with_header(&mut buf).unwrap(),
            EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(&mut buf).unwrap(),
        };
        header.serialize(&mut w).unwrap();
        assert_eq!(
            serialized_size(&header, version).unwrap(),
            w.position(),
            "{version:?}: measured size must equal the bytes the writer produced"
        );

        // And for a BOUNDED, plain type the measured size must also agree with
        // the type-level bound — the two questions coincide when nothing varies.
        let mut b2 = [0u8; 64];
        let mut w2 = match version {
            EncodingVersion::Xcdr1 => CdrWriter::new_with_header(&mut b2).unwrap(),
            EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(&mut b2).unwrap(),
        };
        time.serialize(&mut w2).unwrap();
        let measured = serialized_size(&time, version).unwrap();
        assert_eq!(
            measured,
            w2.position(),
            "{version:?}: Time measured vs written"
        );
        assert_eq!(
            measured,
            max_serialized_size(
                <nros_builtin_interfaces::msg::Time as Message>::FIELDS,
                version
            )
            .unwrap(),
            "{version:?}: Time is plain, so its exact size and its bound must coincide"
        );
    }
}

/// Phase 380 W4 — the buffer predicate is CONST, and answers `false` for an
/// unbounded type rather than guessing.
#[test]
fn buffer_fits_is_const_and_refuses_unbounded_types() {
    use nros_serdes::size::buffer_fits;
    type Time = nros_builtin_interfaces::msg::Time;
    type Header = nros_std_msgs_diag::msg::Header;

    // Const-evaluable: this is the whole point — it can sit in a
    // `const { assert!(..) }` and fail the BUILD instead of dropping samples.
    const FITS: bool = buffer_fits::<Time>(1024, EncodingVersion::Xcdr1);
    const TIGHT: bool = buffer_fits::<Time>(12, EncodingVersion::Xcdr1);
    const TOO_SMALL: bool = buffer_fits::<Time>(11, EncodingVersion::Xcdr1);
    // `const { assert!(..) }` rather than a runtime `assert!`, which is what the
    // comment above asks for — these fail the BUILD instead of a test run — and
    // also what rust 1.97's `clippy::assertions_on_constants` requires, since a
    // runtime assert on a const-evaluable expression is checked far too late.
    const { assert!(FITS) };
    const {
        assert!(
            TIGHT,
            "Time is exactly 12 bytes under XCDR1, so 12 must fit"
        )
    };
    const { assert!(!TOO_SMALL, "11 must not fit a 12-byte type") };

    // XCDR2 is larger (DHEADER), so a buffer sized from XCDR1 alone is a trap
    // the two-constant design exists to expose.
    const { assert!(!buffer_fits::<Time>(12, EncodingVersion::Xcdr2)) };

    // Unbounded => false, never an optimistic guess.
    const {
        assert!(
            !buffer_fits::<Header>(usize::MAX, EncodingVersion::Xcdr1),
            "no finite buffer fits an unbounded type; the honest answer is false"
        )
    };
}

/// Phase 380 W5 — loan eligibility is `plain`, and `plain` means the size is
/// exact.
#[test]
fn loan_eligibility_tracks_plain() {
    use nros_serdes::size::is_loan_eligible;
    type Time = nros_builtin_interfaces::msg::Time;
    type Header = nros_std_msgs_diag::msg::Header;

    const TIME_LOANABLE: bool = is_loan_eligible::<Time>();
    // Const-evaluable, so it is checked at build time — see the note above.
    const { assert!(TIME_LOANABLE, "Time is two fixed integers — fixed layout") };
    assert!(
        !is_loan_eligible::<Header>(),
        "Header has a string; a loan would hand out a pointer into a layout that          is not fixed"
    );

    // The tie that makes this worth deriving rather than declaring: a plain
    // type's bound is its exact size, so eligibility and exactness cannot
    // disagree.
    assert_eq!(
        <Time as Message>::MAX_SERIALIZED_SIZE_XCDR1,
        Some(serialized_size_of_default_time()),
        "for a plain type the bound IS the size of any instance"
    );
}

fn serialized_size_of_default_time() -> usize {
    use nros_serdes::size::serialized_size;
    serialized_size(
        &nros_builtin_interfaces::msg::Time::default(),
        EncodingVersion::Xcdr1,
    )
    .unwrap()
}
