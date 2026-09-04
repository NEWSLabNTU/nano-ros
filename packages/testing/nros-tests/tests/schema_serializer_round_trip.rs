//! phase-421 W5 — the schema-driven strategy round-trips every generated
//! message, with no generated code (RFC-0088 D7).
//!
//! The claim `impl = "schema"` makes is that a provider writes ONE
//! implementation and gets every message. That claim is only worth anything if
//! it is measured against the messages the tree actually has, so this sweeps
//! all of `packages/interfaces/*` — every `impl ::nros_serdes::Message`, and
//! `corpus_is_exhaustive` counts them out of the sources so a message added
//! later cannot silently sit outside the sweep.
//!
//! # Why the message never becomes a Rust value here
//!
//! Not laziness — the schema does not permit it. `SchemaSerializer` takes a
//! `CdrReader`, not the `*const u8` RFC-0088 D7 sketched, because a
//! `String` field's host type is `heapless::String<N>` with `N` absent from the
//! schema, `heapless`' containers are `repr(Rust)`, and a nested type's size is
//! not recorded. `nros_serdes::walk`'s module docs carry the full finding. So
//! the walk transcodes CDR to `packed` and back, and what this file asserts is
//! that the two CDR streams are IDENTICAL — a lossless walk, byte for byte.
//!
//! Driving the CDR side from the schema rather than from 76 hand-written
//! literals is the same trade `serialized_size_bound.rs` made in phase-380: the
//! literals would rot, and rotted literals silently shrink coverage.
//! `a_real_generated_message_survives_the_round_trip` pins the other end — a
//! real value through the type's OWN generated `serialize`/`deserialize` — so
//! the synthetic writer cannot drift from what a publisher runs.
//!
//! Both encodings, because a walk that mishandles the XCDR2 DHEADER passes an
//! XCDR1-only suite.

use std::fs;

use nros_serdes::{
    cdr::{CdrReader, CdrWriter, EncodingVersion},
    schema::{Field, FieldType, Message},
    walk::{SchemaError, SchemaSerializer},
};
use nros_serdes_packed::Packed;

/// One corpus row: a label for failure messages, the ROS type name, the schema.
type Row = (&'static str, &'static str, &'static [Field]);

macro_rules! entry {
    ($t:ty) => {
        (
            stringify!($t),
            <$t as Message>::TYPE_NAME,
            <$t as Message>::FIELDS,
        )
    };
}

/// Every committed generated message in `packages/interfaces/*`.
///
/// Generated once from the sources and pinned by `corpus_is_exhaustive`, which
/// re-counts them at run time — so this list cannot fall behind the tree
/// without a red test naming the difference.
fn corpus() -> Vec<Row> {
    vec![
        entry!(nros_builtin_interfaces::msg::Duration),
        entry!(nros_builtin_interfaces::msg::Time),
        entry!(nros_builtin_interfaces_diag::msg::Duration),
        entry!(nros_builtin_interfaces_diag::msg::Time),
        entry!(nros_diagnostic_msgs::msg::DiagnosticArray),
        entry!(nros_diagnostic_msgs::msg::DiagnosticStatus),
        entry!(nros_diagnostic_msgs::msg::KeyValue),
        entry!(nros_diagnostic_msgs::srv::AddDiagnosticsRequest),
        entry!(nros_diagnostic_msgs::srv::AddDiagnosticsResponse),
        entry!(nros_diagnostic_msgs::srv::SelfTestRequest),
        entry!(nros_diagnostic_msgs::srv::SelfTestResponse),
        entry!(nros_lifecycle_msgs::msg::State),
        entry!(nros_lifecycle_msgs::msg::Transition),
        entry!(nros_lifecycle_msgs::msg::TransitionDescription),
        entry!(nros_lifecycle_msgs::msg::TransitionEvent),
        entry!(nros_lifecycle_msgs::srv::ChangeStateRequest),
        entry!(nros_lifecycle_msgs::srv::ChangeStateResponse),
        entry!(nros_lifecycle_msgs::srv::GetAvailableStatesRequest),
        entry!(nros_lifecycle_msgs::srv::GetAvailableStatesResponse),
        entry!(nros_lifecycle_msgs::srv::GetAvailableTransitionsRequest),
        entry!(nros_lifecycle_msgs::srv::GetAvailableTransitionsResponse),
        entry!(nros_lifecycle_msgs::srv::GetStateRequest),
        entry!(nros_lifecycle_msgs::srv::GetStateResponse),
        entry!(nros_rcl_interfaces::msg::FloatingPointRange),
        entry!(nros_rcl_interfaces::msg::IntegerRange),
        entry!(nros_rcl_interfaces::msg::ListParametersResult),
        entry!(nros_rcl_interfaces::msg::Log),
        entry!(nros_rcl_interfaces::msg::Parameter),
        entry!(nros_rcl_interfaces::msg::ParameterDescriptor),
        entry!(nros_rcl_interfaces::msg::ParameterEvent),
        entry!(nros_rcl_interfaces::msg::ParameterEventDescriptors),
        entry!(nros_rcl_interfaces::msg::ParameterType),
        entry!(nros_rcl_interfaces::msg::ParameterValue),
        entry!(nros_rcl_interfaces::msg::SetParametersResult),
        entry!(nros_rcl_interfaces::srv::DescribeParametersRequest),
        entry!(nros_rcl_interfaces::srv::DescribeParametersResponse),
        entry!(nros_rcl_interfaces::srv::GetParameterTypesRequest),
        entry!(nros_rcl_interfaces::srv::GetParameterTypesResponse),
        entry!(nros_rcl_interfaces::srv::GetParametersRequest),
        entry!(nros_rcl_interfaces::srv::GetParametersResponse),
        entry!(nros_rcl_interfaces::srv::ListParametersRequest),
        entry!(nros_rcl_interfaces::srv::ListParametersResponse),
        entry!(nros_rcl_interfaces::srv::SetParametersAtomicallyRequest),
        entry!(nros_rcl_interfaces::srv::SetParametersAtomicallyResponse),
        entry!(nros_rcl_interfaces::srv::SetParametersRequest),
        entry!(nros_rcl_interfaces::srv::SetParametersResponse),
        entry!(nros_std_msgs_diag::msg::Bool),
        entry!(nros_std_msgs_diag::msg::Byte),
        entry!(nros_std_msgs_diag::msg::ByteMultiArray),
        entry!(nros_std_msgs_diag::msg::Char),
        entry!(nros_std_msgs_diag::msg::ColorRGBA),
        entry!(nros_std_msgs_diag::msg::Empty),
        entry!(nros_std_msgs_diag::msg::Float32),
        entry!(nros_std_msgs_diag::msg::Float32MultiArray),
        entry!(nros_std_msgs_diag::msg::Float64),
        entry!(nros_std_msgs_diag::msg::Float64MultiArray),
        entry!(nros_std_msgs_diag::msg::Header),
        entry!(nros_std_msgs_diag::msg::Int16),
        entry!(nros_std_msgs_diag::msg::Int16MultiArray),
        entry!(nros_std_msgs_diag::msg::Int32),
        entry!(nros_std_msgs_diag::msg::Int32MultiArray),
        entry!(nros_std_msgs_diag::msg::Int64),
        entry!(nros_std_msgs_diag::msg::Int64MultiArray),
        entry!(nros_std_msgs_diag::msg::Int8),
        entry!(nros_std_msgs_diag::msg::Int8MultiArray),
        entry!(nros_std_msgs_diag::msg::MultiArrayDimension),
        entry!(nros_std_msgs_diag::msg::MultiArrayLayout),
        entry!(nros_std_msgs_diag::msg::String),
        entry!(nros_std_msgs_diag::msg::UInt16),
        entry!(nros_std_msgs_diag::msg::UInt16MultiArray),
        entry!(nros_std_msgs_diag::msg::UInt32),
        entry!(nros_std_msgs_diag::msg::UInt32MultiArray),
        entry!(nros_std_msgs_diag::msg::UInt64),
        entry!(nros_std_msgs_diag::msg::UInt64MultiArray),
        entry!(nros_std_msgs_diag::msg::UInt8),
        entry!(nros_std_msgs_diag::msg::UInt8MultiArray),
    ]
}

// ── A populated CDR instance, written from the schema ────────────────────────

/// How many elements a variable-length member gets.
///
/// Two, not one: a stride bug in the walk (reading an element and then
/// re-reading it, or skipping one) is invisible at length 1. Two also stays
/// inside every `heapless` capacity codegen emits, which matters for
/// `a_real_generated_message_survives_the_round_trip`.
const SEQ_LEN: usize = 2;

/// Values vary with a counter so a walk that visits fields out of order, or
/// reuses a value, produces a different byte stream. A constant filler would
/// make a field-swap bug invisible.
struct Counter(u64);

impl Counter {
    fn next(&mut self) -> u64 {
        // A small LCG. Deterministic, so a failure reproduces exactly.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

/// Write a populated instance of `fields`, or `None` when the schema holds a
/// shape the CDR codec cannot write.
fn write_populated(
    fields: &[Field],
    version: EncodingVersion,
    buf: &mut [u8],
) -> Option<(usize, &'static str)> {
    let mut w = match version {
        EncodingVersion::Xcdr1 => CdrWriter::new_with_header(buf).ok()?,
        EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(buf).ok()?,
    };
    let mut c = Counter(0x5eed);
    let dh = w.begin_dheader().ok()?;
    let unsupported = write_fields(&mut w, fields, &mut c)?;
    w.end_dheader(dh).ok()?;
    Some((w.position(), unsupported))
}

/// `Some("")` on success; `Some(reason)` when a shape was refused. `None` only
/// for a writer error, which is a bug in this file (buffer too small).
fn write_fields(w: &mut CdrWriter<'_>, fields: &[Field], c: &mut Counter) -> Option<&'static str> {
    for f in fields {
        let refused = write_one(w, &f.ty, c)?;
        if !refused.is_empty() {
            return Some(refused);
        }
    }
    Some("")
}

fn write_one(w: &mut CdrWriter<'_>, ty: &FieldType, c: &mut Counter) -> Option<&'static str> {
    let n = c.next();
    match ty {
        FieldType::Bool => w.write_bool(n & 1 == 1).ok()?,
        FieldType::Uint8 => w.write_u8(n as u8).ok()?,
        FieldType::Int8 => w.write_i8(n as i8).ok()?,
        FieldType::Uint16 => w.write_u16(n as u16).ok()?,
        FieldType::Int16 => w.write_i16(n as i16).ok()?,
        FieldType::Uint32 => w.write_u32(n as u32).ok()?,
        FieldType::Int32 => w.write_i32(n as i32).ok()?,
        FieldType::Uint64 => w.write_u64(n).ok()?,
        FieldType::Int64 => w.write_i64(n as i64).ok()?,
        // Finite and exactly representable, so a bit-pattern comparison is a
        // fair test of the codec rather than of float formatting.
        FieldType::Float32 => w.write_f32((n % 1000) as f32 * 0.5).ok()?,
        FieldType::Float64 => w.write_f64((n % 1000) as f64 * 0.25).ok()?,
        FieldType::String => w.write_string(&"xy".repeat(1 + (n % 3) as usize)).ok()?,
        FieldType::BoundedString(cap) => {
            let len = (*cap).min(4);
            w.write_string(&"z".repeat(len)).ok()?
        }
        FieldType::WString | FieldType::BoundedWString(_) => {
            return Some("wstring: the CDR codec has no wide-string primitive");
        }
        FieldType::Nested(nested) => {
            let dh = w.begin_dheader().ok()?;
            let refused = write_fields(w, nested.fields, c)?;
            if !refused.is_empty() {
                return Some(refused);
            }
            w.end_dheader(dh).ok()?
        }
        FieldType::Array(len, inner) => {
            for _ in 0..*len {
                let refused = write_one(w, inner, c)?;
                if !refused.is_empty() {
                    return Some(refused);
                }
            }
        }
        FieldType::Sequence(inner) => {
            w.write_sequence_len(SEQ_LEN).ok()?;
            for _ in 0..SEQ_LEN {
                let refused = write_one(w, inner, c)?;
                if !refused.is_empty() {
                    return Some(refused);
                }
            }
        }
        FieldType::BoundedSequence(cap, inner) => {
            let len = (*cap).min(SEQ_LEN);
            w.write_sequence_len(len).ok()?;
            for _ in 0..len {
                let refused = write_one(w, inner, c)?;
                if !refused.is_empty() {
                    return Some(refused);
                }
            }
        }
    }
    Some("")
}

// ── The sweep ────────────────────────────────────────────────────────────────

/// THE property: CDR -> `packed` -> CDR is the identity, for every generated
/// message, in both encodings, using nothing but the `&'static [Field]` schema.
#[test]
fn every_generated_message_round_trips_through_the_schema_walk() {
    let mut cdr_in = vec![0u8; 1 << 16];
    let mut foreign = vec![0u8; 1 << 16];
    let mut cdr_out = vec![0u8; 1 << 16];

    let mut checked = 0usize;
    let mut refused: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for (label, type_name, fields) in corpus() {
        for version in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let (in_len, why) = write_populated(fields, version, &mut cdr_in)
                .unwrap_or_else(|| panic!("{label} {version:?}: could not write a CDR instance"));
            if !why.is_empty() {
                refused.push(format!("{label} {version:?}: {why}"));
                continue;
            }

            // Out: the payload past the encapsulation header is what a
            // publisher hands the transport, and what the walk consumes.
            let mut reader = CdrReader::new_with_header(&cdr_in[..in_len]).unwrap();
            let packed_len = match Packed::serialize(&mut reader, type_name, fields, &mut foreign) {
                Ok(n) => n,
                Err(SchemaError::Unsupported(why)) => {
                    refused.push(format!("{label} {version:?}: {why}"));
                    continue;
                }
                Err(e) => {
                    failures.push(format!("{label} {version:?}: serialize failed: {e}"));
                    continue;
                }
            };

            // The transcode must consume the whole message and nothing more.
            if reader.remaining() != 0 {
                failures.push(format!(
                    "{label} {version:?}: the walk left {} CDR byte(s) unread — it \
                     read a different shape than the writer wrote",
                    reader.remaining()
                ));
                continue;
            }

            // Back.
            let mut w = match version {
                EncodingVersion::Xcdr1 => CdrWriter::new_with_header(&mut cdr_out).unwrap(),
                EncodingVersion::Xcdr2 => CdrWriter::new_with_header_xcdr2(&mut cdr_out).unwrap(),
            };
            let consumed =
                match Packed::deserialize(&foreign[..packed_len], type_name, fields, &mut w) {
                    Ok(n) => n,
                    Err(e) => {
                        failures.push(format!("{label} {version:?}: deserialize failed: {e}"));
                        continue;
                    }
                };
            let out_len = w.position();

            if consumed != packed_len {
                failures.push(format!(
                    "{label} {version:?}: decode consumed {consumed} of {packed_len} \
                     packed byte(s) — the two halves disagree about the wire"
                ));
                continue;
            }
            if cdr_in[..in_len] != cdr_out[..out_len] {
                failures.push(format!(
                    "{label} {version:?}: round trip changed the CDR ({in_len} -> \
                     {out_len} bytes)"
                ));
                continue;
            }
            checked += 1;
        }
    }

    // Preconditions, not decoration. An empty corpus, or one where every row
    // was refused, would make the assertion below vacuous.
    assert!(
        checked >= 150,
        "expected at least 150 (type, encoding) pairs to round-trip — 76 types \
         times two encodings, minus any refused shape — got {checked}, refused \
         {}. The corpus or the walk has silently stopped covering things.",
        refused.len()
    );
    assert!(
        failures.is_empty(),
        "{} round-trip failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // A refusal is a FINDING, not a pass: nothing in packages/interfaces uses a
    // wstring, so anything refused here is new information and must be read.
    assert!(
        refused.is_empty(),
        "{} shape(s) the schema walk could not express — this is a finding \
         about the schema, not a tolerated skip:\n{}",
        refused.len(),
        refused.join("\n")
    );
    eprintln!("schema walk: {checked} (type, encoding) pair(s) round-tripped");
}

/// The corpus above is a hand-maintained list, and a hand-maintained list of
/// what to test is exactly the thing that goes stale. Count the real ones.
#[test]
fn corpus_is_exhaustive() {
    let root = nros_tests::project_root().join("packages/interfaces");
    assert!(
        root.is_dir(),
        "packages/interfaces missing at {} — this test cannot answer its \
         question without the sources",
        root.display()
    );

    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).expect("read_dir") {
            let p = e.expect("dir entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = fs::read_to_string(&p).expect("read source");
                for line in text.lines() {
                    if let Some(rest) = line.strip_prefix("impl ::nros_serdes::Message for ") {
                        let ty = rest.trim_end_matches(" {").trim();
                        found.push(format!("{ty} ({})", p.display()));
                    }
                }
            }
        }
    }

    assert!(
        !found.is_empty(),
        "no `impl ::nros_serdes::Message` found under packages/interfaces — the \
         probe is broken, and a broken probe would make this test vacuous"
    );
    assert_eq!(
        found.len(),
        corpus().len(),
        "the corpus has {} row(s) but packages/interfaces defines {} \
         `impl ::nros_serdes::Message`. Regenerate the corpus — a message \
         outside it is a message the round-trip sweep never sees.\nFound:\n{}",
        corpus().len(),
        found.len(),
        found.join("\n")
    );
}

/// The other end of the trade the sweep makes: a REAL value, through the type's
/// own generated `serialize` / `deserialize`, so the synthetic CDR writer above
/// cannot drift from what a publisher actually runs.
///
/// `DiagnosticStatus` is the shape that exercises everything at once — three
/// strings, a sequence of a nested struct that itself holds two strings.
#[test]
fn a_real_generated_message_survives_the_round_trip() {
    use nros_diagnostic_msgs::msg::{DiagnosticStatus, KeyValue};
    use nros_serdes::traits::{Deserialize, Serialize};

    // Built through `Default` + `push_str` rather than by naming
    // `heapless::String` — this crate does not depend on `heapless`, and a
    // test is not a reason to grow the dependency graph.
    let mut original = DiagnosticStatus {
        level: 2,
        ..Default::default()
    };
    original.name.push_str("motor_left").unwrap();
    original.message.push_str("over temperature").unwrap();
    original.hardware_id.push_str("mcu-7").unwrap();
    for (k, v) in [("temp_c", "91.5"), ("rpm", "0")] {
        let mut kv = KeyValue::default();
        kv.key.push_str(k).unwrap();
        kv.value.push_str(v).unwrap();
        original.values.push(kv).unwrap();
    }

    let mut cdr = [0u8; 1024];
    let mut w = CdrWriter::new_with_header(&mut cdr).unwrap();
    original.serialize(&mut w).unwrap();
    let cdr_len = w.position();

    let mut foreign = [0u8; 1024];
    let mut r = CdrReader::new_with_header(&cdr[..cdr_len]).unwrap();
    let packed_len = Packed::serialize_message::<DiagnosticStatus>(&mut r, &mut foreign).unwrap();

    // The wire really is different — otherwise this test would pass on a
    // transcoder that copied its input.
    assert_ne!(
        &foreign[..packed_len],
        &cdr[..cdr_len],
        "packed and CDR must not coincide, or nothing here is being tested"
    );

    let mut back = [0u8; 1024];
    let mut w2 = CdrWriter::new_with_header(&mut back).unwrap();
    Packed::deserialize_message::<DiagnosticStatus>(&foreign[..packed_len], &mut w2).unwrap();
    let back_len = w2.position();

    assert_eq!(
        &cdr[..cdr_len],
        &back[..back_len],
        "CDR bytes must be identical"
    );

    let mut r2 = CdrReader::new_with_header(&back[..back_len]).unwrap();
    let decoded = DiagnosticStatus::deserialize(&mut r2).unwrap();
    assert_eq!(
        decoded, original,
        "the value must survive, not just the bytes"
    );
}

/// A provider gets the schema, and the schema alone. If `nros_serdes_packed`
/// ever needed a generated per-message item, this would stop compiling — the
/// only thing named here is the trait and the two `&'static` consts.
#[test]
fn the_provider_needs_no_generated_code() {
    fn transcode<M: Message>(cdr: &[u8], out: &mut [u8]) -> Result<usize, SchemaError> {
        let mut r = CdrReader::new_with_header(cdr).unwrap();
        Packed::serialize(&mut r, M::TYPE_NAME, M::FIELDS, out)
    }

    let mut cdr = [0u8; 64];
    let mut w = CdrWriter::new_with_header(&mut cdr).unwrap();
    nros_serdes::traits::Serialize::serialize(
        &nros_builtin_interfaces::msg::Time {
            sec: -3,
            nanosec: 7,
        },
        &mut w,
    )
    .unwrap();
    let len = w.position();

    let mut out = [0u8; 64];
    let n = transcode::<nros_builtin_interfaces::msg::Time>(&cdr[..len], &mut out).unwrap();
    assert_eq!(
        &out[..n],
        &[0xFD, 0xFF, 0xFF, 0xFF, 7, 0, 0, 0],
        "two little-endian words, no padding, no header"
    );
}
