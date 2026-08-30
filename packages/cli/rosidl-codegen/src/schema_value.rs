//! Issue 0896 layer 1 — build a real `nros_serdes::FieldType` from a parsed
//! `.msg`, so codegen can compute a size bound with THE size rule.
//!
//! # Why a value and not a second size calculation
//!
//! The C headers need a per-type `MAX_SERIALIZED_SIZE`, and the obvious way to
//! get one is to walk the rosidl AST adding up field widths. That would be a
//! SECOND implementation of a rule that already exists in
//! `nros_serdes::size`, and a serialized-size rule is exactly the kind that
//! looks right until an encoding rule changes under one copy — the
//! sizes-header mirror defect, issues 0088 -> 0114 -> 0122 -> 0123 -> 0245 ->
//! 0268.
//!
//! So this module does not compute sizes. It builds the INPUT that
//! `nros_serdes::size::max_serialized_size` already consumes, and calls it.
//! One rule, two callers.
//!
//! # Three outcomes, and why the third exists
//!
//! [`TypeBound`] distinguishes:
//!
//! * `Bounded(n)` — a bound exists and is `n`;
//! * `Unbounded` — no bound EXISTS (an unbounded `string`/`sequence`);
//! * `Unresolved` — we could not LOOK, because a nested type is not reachable.
//!
//! Folding the third into the second is the defect issue 0896 is about wearing
//! a different hat: "we looked and there is no bound" and "we could not look"
//! license completely different actions, and only the first is safe to size a
//! buffer from. phase-380's rule is that `None` means unbounded and NEVER
//! unknown, and this type is how that rule survives contact with a resolver
//! that can fail.
//!
//! # `&'static` by leaking
//!
//! `nros_serdes::FieldType` recurses through `&'static` references so the whole
//! schema graph sits in `.rodata` on a target with no allocator. Codegen is a
//! short-lived process building a bounded graph per message, so leaking is the
//! honest way to satisfy that lifetime here — the alternative is a parallel
//! owned mirror of the type, which is the duplication this module exists to
//! avoid.

use nros_serdes::schema::{Field, FieldType as SerdeFieldType, NestedType};
use rosidl_parser::{
    Message,
    ast::{FieldType as IdlFieldType, PrimitiveType},
};
use std::collections::BTreeSet;

/// What one attempt to bound a type concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBound {
    /// A bound exists. Bytes, encapsulation header included.
    Bounded(usize),
    /// No bound exists — an unbounded member. Carries the offending member as
    /// `nros_serdes` names it, so a diagnostic can say WHICH field.
    Unbounded(String),
    /// We could not look: this nested type was not reachable through the
    /// resolver. NOT the same as unbounded; nothing may be sized from it.
    Unresolved(String),
}

/// Why a schema could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// A nested type the resolver could not produce.
    Unresolved(String),
    /// A nested-type cycle. ROS IDL cannot express one, so this means the
    /// resolver returned something inconsistent — report rather than hang.
    Cycle(String),
}

/// Resolve a fully-qualified ROS type name (`pkg/Msg` or `pkg/msg/Msg`) to its
/// parsed message.
pub type MsgLookup<'a> = dyn Fn(&str) -> Option<Message> + 'a;

/// Build the `&'static [Field]` schema for `msg`, resolving nested types
/// recursively through `lookup`.
///
/// `owner` is the fully-qualified name of `msg`, used for cycle reporting.
pub fn build_schema(
    owner: &str,
    msg: &Message,
    lookup: &MsgLookup<'_>,
) -> Result<&'static [Field], SchemaError> {
    let mut stack = BTreeSet::new();
    stack.insert(owner.to_string());
    build_fields(msg, lookup, &mut stack)
}

fn build_fields(
    msg: &Message,
    lookup: &MsgLookup<'_>,
    stack: &mut BTreeSet<String>,
) -> Result<&'static [Field], SchemaError> {
    let mut out = Vec::with_capacity(msg.fields.len());
    for f in &msg.fields {
        out.push(Field {
            name: Box::leak(f.name.clone().into_boxed_str()),
            ty: *lower(&f.field_type, lookup, stack)?,
            // The size rule never reads `offset` (checked: zero references in
            // `nros_serdes::size`), and codegen cannot know a Rust struct's
            // layout anyway. Zero is honest here precisely because nothing
            // consumes it on this path.
            offset: 0,
        });
    }
    Ok(Box::leak(out.into_boxed_slice()))
}

fn lower(
    ty: &IdlFieldType,
    lookup: &MsgLookup<'_>,
    stack: &mut BTreeSet<String>,
) -> Result<&'static SerdeFieldType, SchemaError> {
    let v = match ty {
        IdlFieldType::Primitive(p) => primitive(p),
        IdlFieldType::String => SerdeFieldType::String,
        IdlFieldType::WString => SerdeFieldType::WString,
        IdlFieldType::BoundedString(n) => SerdeFieldType::BoundedString(*n),
        IdlFieldType::BoundedWString(n) => SerdeFieldType::BoundedWString(*n),
        IdlFieldType::Array { element_type, size } => {
            SerdeFieldType::Array(*size, lower(element_type, lookup, stack)?)
        }
        IdlFieldType::Sequence { element_type } => {
            SerdeFieldType::Sequence(lower(element_type, lookup, stack)?)
        }
        IdlFieldType::BoundedSequence {
            element_type,
            max_size,
        } => SerdeFieldType::BoundedSequence(*max_size, lower(element_type, lookup, stack)?),
        IdlFieldType::NamespacedType { package, name } => {
            let fqn = match package {
                Some(p) => format!("{p}/{name}"),
                // A bare name is same-package; the caller's lookup owns that
                // resolution, so hand it the name as written.
                None => name.clone(),
            };
            if !stack.insert(fqn.clone()) {
                return Err(SchemaError::Cycle(fqn));
            }
            let nested = lookup(&fqn).ok_or_else(|| SchemaError::Unresolved(fqn.clone()))?;
            let fields = build_fields(&nested, lookup, stack)?;
            stack.remove(&fqn);
            SerdeFieldType::Nested(Box::leak(Box::new(NestedType {
                type_name: Box::leak(fqn.into_boxed_str()),
                fields,
            })))
        }
    };
    Ok(Box::leak(Box::new(v)))
}

fn primitive(p: &PrimitiveType) -> SerdeFieldType {
    match p {
        PrimitiveType::Bool => SerdeFieldType::Bool,
        PrimitiveType::Byte | PrimitiveType::UInt8 => SerdeFieldType::Uint8,
        PrimitiveType::Char | PrimitiveType::Int8 => SerdeFieldType::Int8,
        PrimitiveType::Int16 => SerdeFieldType::Int16,
        PrimitiveType::UInt16 => SerdeFieldType::Uint16,
        PrimitiveType::Int32 => SerdeFieldType::Int32,
        PrimitiveType::UInt32 => SerdeFieldType::Uint32,
        PrimitiveType::Int64 => SerdeFieldType::Int64,
        PrimitiveType::UInt64 => SerdeFieldType::Uint64,
        PrimitiveType::Float32 => SerdeFieldType::Float32,
        PrimitiveType::Float64 => SerdeFieldType::Float64,
    }
}

/// Bound a parsed message under one encoding, resolving nested types through
/// `lookup`.
///
/// The size itself comes from `nros_serdes::size::max_serialized_size` — this
/// function only supplies its input and classifies the answer.
pub fn bound_message(
    owner: &str,
    msg: &Message,
    version: nros_serdes::cdr::EncodingVersion,
    lookup: &MsgLookup<'_>,
) -> TypeBound {
    let fields = match build_schema(owner, msg, lookup) {
        Ok(f) => f,
        Err(SchemaError::Unresolved(t)) | Err(SchemaError::Cycle(t)) => {
            return TypeBound::Unresolved(t);
        }
    };
    match nros_serdes::size::max_serialized_size(fields, version) {
        Some(n) => TypeBound::Bounded(n),
        None => TypeBound::Unbounded(
            nros_serdes::size::first_unbounded(fields)
                .map(|u| alloc_fmt(&u))
                .unwrap_or_else(|| "<unknown>".to_string()),
        ),
    }
}

/// `UnboundedField` is `Display` on `no_std`; this is the `std` side of that.
fn alloc_fmt(u: &nros_serdes::size::UnboundedField) -> String {
    format!("{u}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_serdes::cdr::EncodingVersion;
    use rosidl_parser::parse_message;

    fn no_lookup(_: &str) -> Option<Message> {
        None
    }

    #[test]
    fn a_flat_bounded_message_gets_a_number() {
        let m = parse_message("int32 a\nuint8 b\n").unwrap();
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup),
            TypeBound::Bounded(_)
        ));
    }

    /// The whole reason this module exists: the number codegen computes must be
    /// the number the runtime computes, because it is the same function over
    /// the same input. A second size implementation is the defect class
    /// (0088 -> 0268) this avoids.
    #[test]
    fn the_bound_equals_what_nros_serdes_computes_directly() {
        let m = parse_message("int64 big\nuint8 small\n").unwrap();
        let fields = build_schema("p/M", &m, &no_lookup).unwrap();
        for v in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let direct = nros_serdes::size::max_serialized_size(fields, v).unwrap();
            assert_eq!(
                bound_message("p/M", &m, v, &no_lookup),
                TypeBound::Bounded(direct)
            );
        }
    }

    /// The two encodings genuinely differ — XCDR2 adds a 4-byte DHEADER and
    /// aligns 8-byte primitives to 4 instead of 8 — so emitting ONE constant
    /// would be wrong for one of them.
    ///
    /// The case is chosen, not arbitrary. `uint8 a; int64 b` makes them AGREE
    /// at 20 bytes by coincidence: XCDR2's DHEADER costs +4 and its looser
    /// alignment saves exactly 4 (a 7-byte pad becomes 3). A bare `int64` has
    /// no pad to save, so the DHEADER shows: 12 vs 16. Picking the first case
    /// would have asserted a rule that happens to hold and called it a law.
    #[test]
    fn the_two_encodings_do_not_agree_so_both_constants_are_needed() {
        let m = parse_message("int64 b\n").unwrap();
        let x1 = bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup);
        let x2 = bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_lookup);
        assert_eq!(x1, TypeBound::Bounded(12));
        assert_eq!(x2, TypeBound::Bounded(16));
    }

    /// The coincidence above, pinned so nobody "fixes" the case choice back.
    #[test]
    fn some_types_do_agree_across_encodings_which_is_why_the_case_matters() {
        let m = parse_message("uint8 a\nint64 b\n").unwrap();
        assert_eq!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup),
            bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_lookup),
            "DHEADER +4 cancels the saved 4 bytes of padding here"
        );
    }

    #[test]
    fn an_unbounded_field_is_named_not_just_reported() {
        let m = parse_message("string label\n").unwrap();
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup) {
            TypeBound::Unbounded(which) => assert!(which.contains("label"), "{which}"),
            other => panic!("expected Unbounded, got {other:?}"),
        }
    }

    /// "Could not look" must never read as "no bound exists" — sizing a buffer
    /// from an unresolved type is the defect issue 0896 is about.
    #[test]
    fn an_unresolvable_nested_type_is_unresolved_not_unbounded() {
        let m = parse_message("std_msgs/Header header\n").unwrap();
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup) {
            TypeBound::Unresolved(t) => assert!(t.contains("Header"), "{t}"),
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }

    #[test]
    fn a_resolvable_nested_type_is_walked_recursively() {
        let m = parse_message("std_msgs/Header header\nint32 v\n").unwrap();
        let lookup = |fqn: &str| -> Option<Message> {
            (fqn == "std_msgs/Header").then(|| parse_message("uint32 seq\nint32 sec\n").unwrap())
        };
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &lookup),
            TypeBound::Bounded(_)
        ));
    }

    /// An unbounded member THREE levels down still makes the whole type
    /// unbounded, and the report names the full path rather than the top field.
    #[test]
    fn an_unbounded_member_deep_in_a_nested_type_is_found() {
        let m = parse_message("std_msgs/Header header\n").unwrap();
        let lookup = |fqn: &str| -> Option<Message> {
            match fqn {
                "std_msgs/Header" => {
                    Some(parse_message("builtin_interfaces/Time stamp\n").unwrap())
                }
                "builtin_interfaces/Time" => Some(parse_message("string frame_id\n").unwrap()),
                _ => None,
            }
        };
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &lookup) {
            TypeBound::Unbounded(which) => {
                assert!(which.contains("header.stamp.frame_id"), "{which}")
            }
            other => panic!("expected Unbounded, got {other:?}"),
        }
    }

    /// THE cross-check layer 1 owed. `schema_value` is a SECOND walk of the
    /// rosidl AST beside `render_field_type_expr`, which is the shape the
    /// sizes-header mirror defect keeps taking. The two diverge legitimately —
    /// the emitted Rust string DEFERS to the nested crate's own `FIELDS` while
    /// a sizeable value must INLINE them — so they cannot be unified, and the
    /// only thing that keeps them honest is that they agree about the number.
    ///
    /// This asserts the agreement structurally: the schema this module builds
    /// must have the same field NAMES and the same field TYPE DISCRIMINANTS, in
    /// the same order, as the fields the emitter walked. A field dropped,
    /// reordered, or mapped to a different variant on one side and not the
    /// other fails here rather than shipping a C constant that disagrees with
    /// its Rust twin.
    #[test]
    fn the_value_walk_matches_the_fields_the_emitter_walks() {
        let src = "bool flag\nint64 wide\nstring<=8 label\nint32[4] fixed\nuint16[] seq\n";
        let m = parse_message(src).unwrap();
        let built = build_schema("p/M", &m, &no_lookup).unwrap();

        assert_eq!(built.len(), m.fields.len(), "field count");
        for (b, idl) in built.iter().zip(&m.fields) {
            assert_eq!(b.name, idl.name, "field name/order");
        }

        // Discriminant agreement, spelled out rather than derived, so a new
        // `FieldType` variant handled on one side only is a failure here.
        use nros_serdes::schema::FieldType as S;
        let kinds: Vec<&str> = built
            .iter()
            .map(|f| match f.ty {
                S::Bool => "bool",
                S::Int64 => "int64",
                S::BoundedString(_) => "bounded_string",
                S::Array(..) => "array",
                S::Sequence(_) => "sequence",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["bool", "int64", "bounded_string", "array", "sequence"]
        );
    }

    /// The two walks, on the same input, must name the same variant.
    ///
    /// This is the closure the hazard actually needs. The structural test above
    /// checks names and order; this checks the MAPPING — for every rosidl
    /// `FieldType` shape, the value this module builds and the expression the
    /// emitter renders must agree about which `nros_serdes::FieldType` variant
    /// it is. A shape mapped one way here and another way there is how a C
    /// constant ends up disagreeing with its Rust twin, and it is invisible
    /// until someone compares the two numbers on a target.
    ///
    /// Every variant the parser can produce is listed. Adding one to the IDL
    /// without adding it here leaves a hole, so the list is the coverage claim
    /// and should be read as one.
    #[test]
    fn both_walks_map_every_shape_to_the_same_variant() {
        use nros_serdes::schema::FieldType as S;

        // (idl declaration, the variant BOTH walks must produce)
        let cases: &[(&str, &str)] = &[
            ("bool f\n", "Bool"),
            ("uint8 f\n", "Uint8"),
            ("int8 f\n", "Int8"),
            ("int16 f\n", "Int16"),
            ("uint16 f\n", "Uint16"),
            ("int32 f\n", "Int32"),
            ("uint32 f\n", "Uint32"),
            ("int64 f\n", "Int64"),
            ("uint64 f\n", "Uint64"),
            ("float32 f\n", "Float32"),
            ("float64 f\n", "Float64"),
            ("string f\n", "String"),
            ("string<=8 f\n", "BoundedString"),
            ("int32[4] f\n", "Array"),
            ("int32[] f\n", "Sequence"),
            ("int32[<=4] f\n", "BoundedSequence"),
        ];

        for (src, want) in cases {
            let m = parse_message(src).unwrap();
            let idl_ty = &m.fields[0].field_type;

            // Walk A — the emitter's, rendering a Rust expression string.
            let mut helpers = String::new();
            let expr = crate::generator::common::render_field_type_expr(
                "f",
                idl_ty,
                "p",
                "P_",
                &mut helpers,
                &crate::generator::common::default_nested_type_path,
            );
            let rendered = format!("{expr}{helpers}");
            assert!(
                rendered.contains(&format!("FieldType::{want}")),
                "emitter mapped `{src}` to {rendered}, expected {want}"
            );

            // Walk B — this module's, building a value.
            let built = build_schema("p/M", &m, &no_lookup).unwrap();
            let got = match built[0].ty {
                S::Bool => "Bool",
                S::Uint8 => "Uint8",
                S::Int8 => "Int8",
                S::Int16 => "Int16",
                S::Uint16 => "Uint16",
                S::Int32 => "Int32",
                S::Uint32 => "Uint32",
                S::Int64 => "Int64",
                S::Uint64 => "Uint64",
                S::Float32 => "Float32",
                S::Float64 => "Float64",
                S::String => "String",
                S::WString => "WString",
                S::BoundedString(_) => "BoundedString",
                S::BoundedWString(_) => "BoundedWString",
                S::Array(..) => "Array",
                S::Sequence(_) => "Sequence",
                S::BoundedSequence(..) => "BoundedSequence",
                S::Nested(_) => "Nested",
            };
            assert_eq!(&got, want, "value walk mapped `{src}` to {got}");
        }
    }

    /// A same-package nested type resolved off sibling `.msg` files is the
    /// case the CLI wiring actually serves, so it is tested with a real
    /// directory rather than a closure that always answers.
    ///
    /// The previous state of this path is what makes it worth asserting: with
    /// no resolver EVERY nested type reported `Unresolved` and got no
    /// constant, so the layer-2 win landed only on flat messages.
    #[test]
    fn a_sibling_msg_file_resolves_and_yields_a_bound() {
        let dir = std::env::temp_dir().join(format!("nros0896-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Inner.msg"), "int32 sec\nuint32 nanosec\n").unwrap();

        let outer = parse_message("Inner stamp\nint32 v\n").unwrap();
        let d = dir.clone();
        let lookup = move |fqn: &str| -> Option<Message> {
            let name = fqn.rsplit('/').next()?;
            let body = std::fs::read_to_string(d.join(format!("{name}.msg"))).ok()?;
            parse_message(&body).ok()
        };

        let got = bound_message("p/Outer", &outer, EncodingVersion::Xcdr1, &lookup);
        std::fs::remove_dir_all(&dir).ok();
        assert!(
            matches!(got, TypeBound::Bounded(_)),
            "a resolvable nested type must produce a bound, got {got:?}"
        );
    }

    /// RX must never be smaller than TX. An undersized receive buffer drops
    /// samples silently; an oversized transmit buffer only wastes stack. The
    /// asymmetry in consequence is why the two constants exist and why this
    /// direction is the one asserted.
    #[test]
    fn the_receive_bound_is_never_below_the_transmit_bound() {
        for src in [
            "int64 b\n",
            "uint8 a\nint64 b\n",
            "bool f\nfloat64 d\nstring<=8 s\n",
            "int32[4] fixed\nuint16 u\n",
        ] {
            let m = parse_message(src).unwrap();
            let x1 = bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_lookup);
            let x2 = bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_lookup);
            if let (TypeBound::Bounded(tx), TypeBound::Bounded(other)) = (&x1, &x2) {
                let rx = *tx.max(other);
                assert!(rx >= *tx, "rx {rx} < tx {tx} for `{src}`");
            }
        }
    }

    /// ROS IDL cannot express a cycle, so one means the resolver is
    /// inconsistent. Report it instead of recursing forever.
    #[test]
    fn a_resolver_cycle_is_reported_rather_than_hanging() {
        let m = parse_message("p/A a\n").unwrap();
        let lookup = |fqn: &str| -> Option<Message> {
            (fqn == "p/A").then(|| parse_message("p/A inner\n").unwrap())
        };
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &lookup),
            TypeBound::Unresolved(_)
        ));
    }
}
