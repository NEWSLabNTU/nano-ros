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
use rosidl_lower::config::{CapacityResolver, FieldKind};
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
    /// No bound exists. Carries EVERY offending member as `nros_serdes` names
    /// them, in declaration order, so one build names everything that has to be
    /// bounded (phase-403 W0).
    ///
    /// A `Vec` and not the first offender: with an unbounded type now a build
    /// error, and most stock ROS types unbounded in several places, naming one
    /// member per build makes bounding a package a cap-rebuild-repeat loop with
    /// a full codegen run per member. Never empty — `Unbounded` is only
    /// constructed from a walk that found at least one.
    Unbounded(Vec<String>),
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
/// `owner` is the fully-qualified name of `msg`, used for cycle reporting and
/// as the resolver key of its own fields.
///
/// `caps` is the `nros-codegen.toml` resolver. A field the config caps in a mode
/// that actually holds the cap is lowered to the BOUNDED variant, so the cap
/// reaches the derived bound and not only the storage container (phase-403 W0).
/// Pass [`CapacityResolver::empty`] to bound a type from its `.msg` alone.
pub fn build_schema(
    owner: &str,
    msg: &Message,
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
) -> Result<&'static [Field], SchemaError> {
    let mut stack = BTreeSet::new();
    stack.insert(owner.to_string());
    build_fields(msg, decl_of(owner), caps, lookup, &mut stack)
}

/// The `(package, message)` a type name resolves against as a DECLARING type.
///
/// `nav_msgs/msg/Odometry` and `nav_msgs/Odometry` both give
/// `(Some("nav_msgs"), "Odometry")`; a bare `Odometry` gives `(None, "Odometry")`,
/// which is what the unit tests and any caller without package context pass.
///
/// Package first segment, message LAST: the `.msg` spelling with the `msg/`
/// infix and the spelling without it are both in use across this crate's
/// callers, and keying on "everything after the first slash" would make
/// `nav_msgs/msg/Odometry` and `nav_msgs/Odometry` two different config keys for
/// one type.
fn decl_of(owner: &str) -> (Option<&str>, &str) {
    let package = owner.split('/').next().filter(|p| p.len() < owner.len());
    let message = owner.rsplit('/').next().unwrap_or(owner);
    (package, message)
}

fn build_fields(
    msg: &Message,
    // The DECLARING type of these fields: the package a bare nested reference
    // resolves against (phase-403 W6), and the resolver key the fields' own caps
    // are read with (phase-403 W0). One value, because both questions have the
    // same answer and splitting them is how they drift apart.
    //
    // W6, the package half. ROS `.msg` says a bare `Pose` means "same package",
    // and "same" is the package of the message the field is DECLARED IN, not of
    // whatever top-level message the walk started from. Descending into
    // `geometry_msgs/PoseWithCovariance` from `nav_msgs/msg/Odometry` and then
    // asking a lookup for a bare `Pose` asks it the wrong question: the answer
    // is `geometry_msgs/Pose`, and a lookup keyed on the ORIGINAL package finds
    // nothing and reports `Unresolved`. Latent until W6 only because no caller
    // had a cross-package lookup at all, so the walk never got one level down.
    //
    // W0, the message half. A config entry names the type that DECLARES the
    // field, so descending into `std_msgs/Header` from `nav_msgs/msg/Odometry`
    // must ask about `std_msgs/Header.frame_id`, not
    // `nav_msgs/Odometry.frame_id`. That is what makes ONE cap on
    // `Header.frame_id` bound every message that nests a `Header` — the same
    // mistake W6 fixed on the package half would have made the direct case pass
    // and left every nested `Header` unbounded, so it is tested both ways
    // (`one_cap_on_the_declaring_type_bounds_every_message_that_nests_it` and
    // `a_cap_keyed_on_the_containing_type_does_not_reach_a_nested_field`).
    decl: (Option<&str>, &str),
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
    stack: &mut BTreeSet<String>,
) -> Result<&'static [Field], SchemaError> {
    let (current_package, current_message) = decl;
    let mut out = Vec::with_capacity(msg.fields.len());
    for f in &msg.fields {
        // phase-403 W7 — the ELEMENT dimension, applied first and by REWRITING
        // the shape: a `string[]` whose config states `element_cap = 32` is
        // lowered as the `string<=32[]` the `.msg` could have said, so the arms
        // below need no element case at all. `declared_element_bound` applies
        // the shape / `.msg`-wins / mode rules; an element that gets no stated
        // cap still gets NO bound from the built-in 256, which is the rule W0
        // exists to enforce.
        let field_type = caps.element_capped(
            current_package.unwrap_or_default(),
            current_message,
            &f.name,
            &f.field_type,
        );
        // Only the field's OWN top-level shape consults the config for `cap`,
        // and only for the two shapes the resolver is keyed on — the same
        // `String`/`WString`/`Sequence` set `field_to_nros_field_with_mode`
        // calls configurable.
        let declared = match field_type.as_ref() {
            IdlFieldType::String | IdlFieldType::WString => caps.declared_bound(
                current_package.unwrap_or_default(),
                current_message,
                &f.name,
                FieldKind::String,
            ),
            IdlFieldType::Sequence { .. } => caps.declared_bound(
                current_package.unwrap_or_default(),
                current_message,
                &f.name,
                FieldKind::Sequence,
            ),
            _ => None,
        };
        out.push(Field {
            name: Box::leak(f.name.clone().into_boxed_str()),
            ty: *lower(
                field_type.as_ref(),
                declared,
                current_package,
                caps,
                lookup,
                stack,
            )?,
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
    // phase-403 W0 — the bound the codegen config STATES for this field, or
    // `None`. Precomputed by the caller from the DECLARING type, so the
    // recursion below can pass `None` for element types without needing a rule
    // about which nesting level a config key applies to.
    //
    // A `.msg` bound wins over it by CONSTRUCTION, not by precedence: a bounded
    // shape has its own arm here and never looks at `declared` at all. The
    // interface is authoritative, and a config cap can neither widen nor narrow
    // it.
    declared: Option<usize>,
    current_package: Option<&str>,
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
    stack: &mut BTreeSet<String>,
) -> Result<&'static SerdeFieldType, SchemaError> {
    let sub = |ty: &IdlFieldType,
               stack: &mut BTreeSet<String>|
     -> Result<&'static SerdeFieldType, SchemaError> {
        lower(ty, None, current_package, caps, lookup, stack)
    };
    let v = match ty {
        IdlFieldType::Primitive(p) => primitive(p),
        IdlFieldType::String => match declared {
            Some(n) => SerdeFieldType::BoundedString(n),
            None => SerdeFieldType::String,
        },
        IdlFieldType::WString => match declared {
            Some(n) => SerdeFieldType::BoundedWString(n),
            None => SerdeFieldType::WString,
        },
        IdlFieldType::BoundedString(n) => SerdeFieldType::BoundedString(*n),
        IdlFieldType::BoundedWString(n) => SerdeFieldType::BoundedWString(*n),
        IdlFieldType::Array { element_type, size } => {
            SerdeFieldType::Array(*size, sub(element_type, stack)?)
        }
        IdlFieldType::Sequence { element_type } => {
            let elem = sub(element_type, stack)?;
            match declared {
                Some(n) => SerdeFieldType::BoundedSequence(n, elem),
                None => SerdeFieldType::Sequence(elem),
            }
        }
        IdlFieldType::BoundedSequence {
            element_type,
            max_size,
        } => SerdeFieldType::BoundedSequence(*max_size, sub(element_type, stack)?),
        IdlFieldType::NamespacedType { package, name } => {
            let fqn = match (package, current_package) {
                (Some(p), _) => format!("{p}/{name}"),
                // A bare name is same-package, and "same" is the package of the
                // message this field was declared in.
                (None, Some(p)) => format!("{p}/{name}"),
                // No package context at all: hand the lookup the name as
                // written and let it decide, which is what callers with a
                // single-package view already do.
                (None, None) => name.clone(),
            };
            if !stack.insert(fqn.clone()) {
                return Err(SchemaError::Cycle(fqn));
            }
            let nested = lookup(&fqn).ok_or_else(|| SchemaError::Unresolved(fqn.clone()))?;
            let fields = build_fields(&nested, decl_of(&fqn), caps, lookup, stack)?;
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
/// `lookup` and reading capped fields through `caps`.
///
/// The size itself comes from `nros_serdes::size::max_serialized_size` — this
/// function only supplies its input and classifies the answer.
pub fn bound_message(
    owner: &str,
    msg: &Message,
    version: nros_serdes::cdr::EncodingVersion,
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
) -> TypeBound {
    let fields = match build_schema(owner, msg, caps, lookup) {
        Ok(f) => f,
        Err(SchemaError::Unresolved(t)) | Err(SchemaError::Cycle(t)) => {
            return TypeBound::Unresolved(t);
        }
    };
    match nros_serdes::size::max_serialized_size(fields, version) {
        Some(n) => TypeBound::Bounded(n),
        // EVERY offending member, not the first: with an unbounded type a build
        // error, one build has to name everything the user must bound, or
        // bounding a package becomes one cap and one full codegen run per
        // member (phase-403 W0).
        None => TypeBound::Unbounded(unbounded_members(fields)),
    }
}

/// Every unbounded member of a built schema, formatted as `nros_serdes` names
/// them.
///
/// Never empty for a schema `max_serialized_size` rejected: the two walks agree
/// by construction (`nros_serdes::size` asserts it). The `<unknown>` fallback
/// exists so a hypothetical disagreement produces a diagnostic rather than a
/// reason nobody can read.
fn unbounded_members(fields: &'static [Field]) -> Vec<String> {
    let mut out = Vec::new();
    let _ = nros_serdes::size::visit_unbounded(fields, &mut |u| {
        out.push(format!("{u}"));
        std::ops::ControlFlow::Continue(())
    });
    if out.is_empty() {
        out.push("<unknown>".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_serdes::cdr::EncodingVersion;
    use rosidl_parser::parse_message;

    fn no_caps() -> CapacityResolver {
        CapacityResolver::empty()
    }

    fn no_lookup(_: &str) -> Option<Message> {
        None
    }

    #[test]
    fn a_flat_bounded_message_gets_a_number() {
        let m = parse_message("int32 a\nuint8 b\n").unwrap();
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup),
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
        let fields = build_schema("p/M", &m, &no_caps(), &no_lookup).unwrap();
        for v in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let direct = nros_serdes::size::max_serialized_size(fields, v).unwrap();
            assert_eq!(
                bound_message("p/M", &m, v, &no_caps(), &no_lookup),
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
        let x1 = bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup);
        let x2 = bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_caps(), &no_lookup);
        assert_eq!(x1, TypeBound::Bounded(12));
        assert_eq!(x2, TypeBound::Bounded(16));
    }

    /// The coincidence above, pinned so nobody "fixes" the case choice back.
    #[test]
    fn some_types_do_agree_across_encodings_which_is_why_the_case_matters() {
        let m = parse_message("uint8 a\nint64 b\n").unwrap();
        assert_eq!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup),
            bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_caps(), &no_lookup),
            "DHEADER +4 cancels the saved 4 bytes of padding here"
        );
    }

    #[test]
    fn an_unbounded_field_is_named_not_just_reported() {
        let m = parse_message("string label\n").unwrap();
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup) {
            TypeBound::Unbounded(which) => {
                assert!(which.iter().any(|w| w.contains("label")), "{which:?}")
            }
            other => panic!("expected Unbounded, got {other:?}"),
        }
    }

    /// "Could not look" must never read as "no bound exists" — sizing a buffer
    /// from an unresolved type is the defect issue 0896 is about.
    #[test]
    fn an_unresolvable_nested_type_is_unresolved_not_unbounded() {
        let m = parse_message("std_msgs/Header header\n").unwrap();
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup) {
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
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &lookup),
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
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &lookup) {
            TypeBound::Unbounded(which) => {
                assert!(
                    which.iter().any(|w| w.contains("header.stamp.frame_id")),
                    "{which:?}"
                )
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
        let built = build_schema("p/M", &m, &no_caps(), &no_lookup).unwrap();

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
                None,
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
            let built = build_schema("p/M", &m, &no_caps(), &no_lookup).unwrap();
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

        let got = bound_message(
            "p/Outer",
            &outer,
            EncodingVersion::Xcdr1,
            &no_caps(),
            &lookup,
        );
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
            let x1 = bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup);
            let x2 = bound_message("p/M", &m, EncodingVersion::Xcdr2, &no_caps(), &no_lookup);
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
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &lookup),
            TypeBound::Unresolved(_)
        ));
    }

    // ── phase-403 W0 — a `cap` reaches the derived BOUND, not only storage ──

    fn caps(toml: &str) -> CapacityResolver {
        CapacityResolver::from_toml_str(toml).expect("corpus config parses")
    }

    /// The gap this wave closes. A `cap` selected the storage container and
    /// stopped there, so a type whose only unbounded field was capped still had
    /// NO bound — and phase-403 W0 had just made that a build error, with a
    /// diagnostic telling the user to reach for exactly this knob.
    #[test]
    fn an_inline_cap_gives_the_type_a_bound_the_msg_never_stated() {
        let m = parse_message("string label\n").unwrap();
        let r = caps("[fields]\n\"p/M.label\" = 24\n");
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup) {
            TypeBound::Bounded(n) => {
                // 4 encapsulation + 4 length + 24 payload + 1 NUL.
                assert_eq!(n, 33, "the bound must be the cap the config states");
            }
            other => panic!("expected Bounded, got {other:?}"),
        }
        // Same `.msg`, no config: still unbounded. The cap is the whole
        // difference.
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup),
            TypeBound::Unbounded(_)
        ));
    }

    /// THE transitivity claim, tested rather than assumed.
    ///
    /// The resolver is keyed by the DECLARING type, so ONE entry for
    /// `std_msgs/Header.frame_id` should bound `header` in every message that
    /// nests a `Header` — which is the difference between capping stock ROS once
    /// and capping it once per containing type (12 packages, 120 types).
    ///
    /// Not assumed, because W6 found the sibling bug in this exact walk: a BARE
    /// nested reference resolved against the top-level package instead of the
    /// declaring one. The same mistake here would key the cap on
    /// `robot_msgs/Pose.frame_id`, find nothing, and leave every nested Header
    /// unbounded while the direct case passed.
    ///
    /// Three levels, and the containing messages are in a DIFFERENT package from
    /// the capped one, so a walk that carried the top-level key forward cannot
    /// pass by coincidence.
    #[test]
    fn one_cap_on_the_declaring_type_bounds_every_message_that_nests_it() {
        let lookup = |fqn: &str| -> Option<Message> {
            match fqn {
                "std_msgs/Header" => {
                    Some(parse_message("builtin_interfaces/Time stamp\nstring frame_id\n").unwrap())
                }
                "builtin_interfaces/Time" => {
                    Some(parse_message("int32 sec\nuint32 nanosec\n").unwrap())
                }
                "geometry_msgs/PoseStamped" => {
                    Some(parse_message("std_msgs/Header header\nfloat64 x\n").unwrap())
                }
                _ => None,
            }
        };

        // ONE entry, naming the type that DECLARES the field.
        let r = caps("[fields]\n\"std_msgs/Header.frame_id\" = 32\n");

        // Directly, one level down, and two levels down through a third package.
        for (owner, src) in [
            (
                "std_msgs/msg/Header",
                "builtin_interfaces/Time stamp\nstring frame_id\n",
            ),
            ("robot_msgs/msg/Tagged", "std_msgs/Header header\nint32 v\n"),
            (
                "robot_msgs/msg/Deep",
                "geometry_msgs/PoseStamped pose\nint32 v\n",
            ),
        ] {
            let m = parse_message(src).unwrap();
            let got = bound_message(owner, &m, EncodingVersion::Xcdr1, &r, &lookup);
            assert!(
                matches!(got, TypeBound::Bounded(_)),
                "{owner} must be bounded by the ONE cap on std_msgs/Header.frame_id, got {got:?}"
            );
        }

        // Control: without the entry every one of them is unbounded, so the
        // test above is measuring the cap and not a walk that bounds by
        // accident.
        for (owner, src) in [
            ("robot_msgs/msg/Tagged", "std_msgs/Header header\nint32 v\n"),
            (
                "robot_msgs/msg/Deep",
                "geometry_msgs/PoseStamped pose\nint32 v\n",
            ),
        ] {
            let m = parse_message(src).unwrap();
            assert!(
                matches!(
                    bound_message(owner, &m, EncodingVersion::Xcdr1, &no_caps(), &lookup),
                    TypeBound::Unbounded(_)
                ),
                "{owner} must be unbounded with no config"
            );
        }
    }

    /// A cap keyed on the CONTAINING type must not reach a nested field. The
    /// mirror of the test above: if the walk carried the top-level key down, a
    /// key that names the wrong type would start working, and both tests are
    /// needed to say the key means what it says.
    #[test]
    fn a_cap_keyed_on_the_containing_type_does_not_reach_a_nested_field() {
        let lookup = |fqn: &str| -> Option<Message> {
            (fqn == "std_msgs/Header").then(|| parse_message("string frame_id\n").unwrap())
        };
        let r = caps("[fields]\n\"robot_msgs/Tagged.frame_id\" = 32\n");
        let m = parse_message("std_msgs/Header header\n").unwrap();
        assert!(matches!(
            bound_message(
                "robot_msgs/msg/Tagged",
                &m,
                EncodingVersion::Xcdr1,
                &r,
                &lookup
            ),
            TypeBound::Unbounded(_)
        ));
    }

    /// A `.msg` bound is the interface every participant compiled against; a cap
    /// is this build's own claim. The interface wins, and a cap can neither
    /// widen nor narrow it — asserted in BOTH directions so "wins" cannot be
    /// read as "the larger of the two".
    #[test]
    fn a_msg_bound_wins_over_a_config_cap_in_both_directions() {
        let m = parse_message("string<=8 label\n").unwrap();
        let from_msg = bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup);
        for cap in ["4", "4096"] {
            let r = caps(&format!("[fields]\n\"p/M.label\" = {cap}\n"));
            assert_eq!(
                bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup),
                from_msg,
                "a cap of {cap} moved a bound the .msg already states"
            );
        }
    }

    /// RFC-0033 "What each mode GUARANTEES": only `inline` promises the size is
    /// in the type. A `heap` cap is documented as a hint (`alloc::Vec<T>`, and
    /// `nros_type_for_field_heap` does not even take the cap); a `view` field is
    /// "a slice into the CDR receive buffer, no copy, NO FIXED CAPACITY", read
    /// out with a bare `reader.read_string()?` that checks no length.
    ///
    /// So a cap in either mode is a number nothing enforces, and sizing a
    /// receive buffer from it would be the silent shortfall this phase exists to
    /// remove. Unbounded is the safe answer and it fails at BUILD time.
    #[test]
    fn a_cap_bounds_only_in_the_mode_that_actually_holds_it() {
        let m = parse_message("string label\nint64[] samples\n").unwrap();
        for (mode, bounded) in [("inline", true), ("heap", false), ("view", false)] {
            let r = caps(&format!(
                "[fields]\n\
                 \"p/M.label\"   = {{ cap = 24, mode = \"{mode}\" }}\n\
                 \"p/M.samples\" = {{ cap = 4, mode = \"{mode}\" }}\n"
            ));
            let got = bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup);
            assert_eq!(
                matches!(got, TypeBound::Bounded(_)),
                bounded,
                "mode `{mode}` produced {got:?}"
            );
        }
    }

    /// The built-in 256/64 fallback must NOT read as a bound.
    ///
    /// `CapacityResolver::resolve` always answers, so the naive wiring would
    /// have bounded every unbounded string in the tree at 256 — quietly
    /// satisfying phase-403 W0's rule everywhere and deleting it. A bound has to
    /// be something a human stated.
    #[test]
    fn the_builtin_capacity_default_is_not_a_stated_bound() {
        let m = parse_message("string label\n").unwrap();
        // An unrelated entry, so the resolver is non-empty and the fallthrough
        // path is the one under test.
        let r = caps("[fields]\n\"other/Thing.x\" = 8\n");
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup),
            TypeBound::Unbounded(_)
        ));
    }

    /// A `[defaults]` line IS a stated bound: somebody wrote it in a config
    /// file. Only the level-6 built-in constant is not.
    #[test]
    fn a_defaults_level_cap_is_a_stated_bound() {
        let m = parse_message("string label\n").unwrap();
        let r = caps("[defaults]\nstring = 16\n");
        assert!(matches!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup),
            TypeBound::Bounded(_)
        ));
    }

    /// A capped SEQUENCE OF STRINGS stays unbounded, because the element string
    /// is spelled from a built-in default (`nros_type_for_field_with_mode`) and
    /// not from any config key. Claiming a bound here would be claiming 256
    /// bytes per element that nobody chose.
    ///
    /// Pinned because it is the tempting over-reach: the field HAS a cap, so it
    /// looks bounded, and the resulting number would be wrong in the direction
    /// that under-sizes a buffer if the default ever moved.
    #[test]
    fn capping_a_sequence_of_strings_does_not_bound_its_elements() {
        let m = parse_message("string[] lines\n").unwrap();
        let r = caps("[fields]\n\"p/M.lines\" = 4\n");
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup) {
            TypeBound::Unbounded(which) => assert!(
                which.iter().any(|w| w.contains("lines")),
                "the element, not the sequence, is what is unbounded: {which:?}"
            ),
            other => panic!("expected Unbounded, got {other:?}"),
        }
    }

    /// EVERY offending member, in declaration order — the whole point of the
    /// all-members form. `first_unbounded` would have named `a` and stopped,
    /// which is one cap and one full codegen run per member for a stock ROS
    /// type that has several.
    #[test]
    fn every_unbounded_member_is_reported_not_just_the_first() {
        let m = parse_message("string a\nint32 keep\nint64[] b\nstring c\n").unwrap();
        match bound_message("p/M", &m, EncodingVersion::Xcdr1, &no_caps(), &no_lookup) {
            TypeBound::Unbounded(which) => {
                assert_eq!(
                    which,
                    vec![
                        "a (string)".to_string(),
                        "b (sequence<T>)".to_string(),
                        "c (string)".to_string()
                    ]
                );
            }
            other => panic!("expected Unbounded, got {other:?}"),
        }
    }

    /// Members are reported across NESTED types too, and capping some of them
    /// leaves exactly the rest — so the second build's diagnostic is strictly
    /// shorter, which is what makes the loop terminate.
    #[test]
    fn capping_some_members_leaves_exactly_the_others_named() {
        let lookup = |fqn: &str| -> Option<Message> {
            (fqn == "std_msgs/Header").then(|| parse_message("string frame_id\n").unwrap())
        };
        let m = parse_message("std_msgs/Header header\nstring child_frame_id\n").unwrap();

        let all = bound_message(
            "nav_msgs/msg/Odom",
            &m,
            EncodingVersion::Xcdr1,
            &no_caps(),
            &lookup,
        );
        assert_eq!(
            all,
            TypeBound::Unbounded(vec![
                "header.frame_id (string)".to_string(),
                "child_frame_id (string)".to_string(),
            ])
        );

        let r = caps("[fields]\n\"std_msgs/Header.frame_id\" = 32\n");
        assert_eq!(
            bound_message("nav_msgs/msg/Odom", &m, EncodingVersion::Xcdr1, &r, &lookup),
            TypeBound::Unbounded(vec!["child_frame_id (string)".to_string()])
        );
    }

    /// THE cross-check the cap change owes, and the reason `SchemaCaps` exists.
    ///
    /// Two walks read the config now — this module's (which feeds the C header's
    /// `_TX/_RX_MAX_SERIALIZED_SIZE`) and the emitter's (which feeds the Rust
    /// `Message::FIELDS`, and therefore `M::MAX_SERIALIZED_SIZE_XCDR*` and
    /// `rx_buffer_for!(M)`). If only one of them honoured a cap, capping a field
    /// would fix the C build and leave the Rust build still refusing to compile,
    /// which is the defect wearing a different hat.
    ///
    /// So: the same cap, through both walks, must produce the same VARIANT with
    /// the same number.
    #[test]
    fn a_cap_reaches_the_c_bound_and_the_rust_schema_alike() {
        use crate::generator::common::{SchemaCaps, build_nros_message_schema};

        let src = "string label\nint64[] samples\n";
        let m = parse_message(src).unwrap();
        let r = caps(
            "[fields]\n\
             \"p/M.label\"   = 24\n\
             \"p/M.samples\" = { cap = 6, mode = \"inline\" }\n",
        );

        // Walk A — the value this module builds, which the C constant is
        // computed from.
        let built = build_schema("p/M", &m, &r, &no_lookup).unwrap();
        use nros_serdes::schema::FieldType as S;
        assert!(matches!(built[0].ty, S::BoundedString(24)));
        assert!(matches!(built[1].ty, S::BoundedSequence(6, _)));

        // Walk B — the Rust expression the emitter renders into `FIELDS`.
        let schema = build_nros_message_schema("p", "M", &m.fields, &SchemaCaps::new("M", &r));
        assert!(
            schema.fields_block.contains("FieldType::BoundedString(24)"),
            "{}",
            schema.fields_block
        );
        assert!(
            schema
                .fields_block
                .contains("FieldType::BoundedSequence(6, &FT_SAMPLES_ELEM)"),
            "{}",
            schema.fields_block
        );

        // And with no config both walks fall back together.
        let plain = build_schema("p/M", &m, &no_caps(), &no_lookup).unwrap();
        assert!(matches!(plain[0].ty, S::String));
        let plain_schema =
            build_nros_message_schema("p", "M", &m.fields, &SchemaCaps::unconfigured());
        assert!(plain_schema.fields_block.contains("FieldType::String"));
    }

    // ========================================================================
    // phase-403 W7 — the element dimension
    // ========================================================================

    /// THE claim W7 makes, as one equality: `cap` + `element_cap` on a
    /// `string[]` derives EXACTLY the number the `.msg` spelling of the same two
    /// numbers derives.
    ///
    /// Not "the type is now bounded" — that would pass on a wrong number — and
    /// not a literal, which would have to be recomputed by hand every time the
    /// encoding rules move. Comparing the two paths makes the config a spelling
    /// of the interface rather than a second sizing rule, which is the whole
    /// design: the emitters and `nros_serdes::size` have always handled
    /// `string<=32[<=16]`, and this asserts the config reaches the same shape.
    #[test]
    fn an_element_cap_derives_what_the_msg_spelling_of_it_derives() {
        let configured = parse_message("string[] name\n").unwrap();
        let spelled = parse_message("string<=32[<=16] name\n").unwrap();
        let r = caps("[fields]\n\"p/M.name\" = { cap = 16, element_cap = 32 }\n");
        for v in [EncodingVersion::Xcdr1, EncodingVersion::Xcdr2] {
            let from_config = bound_message("p/M", &configured, v, &r, &no_lookup);
            assert_eq!(
                from_config,
                bound_message("p/M", &spelled, v, &no_caps(), &no_lookup),
                "{v:?}"
            );
            assert!(
                matches!(from_config, TypeBound::Bounded(_)),
                "{from_config:?}"
            );
        }
    }

    /// The framing, spelled out once so the arithmetic is in the tree rather
    /// than in a commit message.
    ///
    /// A CDR string costs a 4-byte length prefix, the payload, and a NUL --
    /// `write_string` writes `len + 1` as the prefix -- so ONE element is
    /// `4 + element_cap + 1`, not `4 + element_cap`, and each element after the
    /// first is padded up to the next multiple of 4. For `cap = 2,
    /// element_cap = 32` under XCDR1 that is the 4-byte encapsulation header,
    /// the sequence's own 4-byte count, one padded element (40) and one
    /// unpadded final element (37): 85.
    ///
    /// Asserted as a literal deliberately. Everything else here compares two
    /// paths, which cannot notice both paths moving together; this notices.
    #[test]
    fn the_per_element_cost_includes_the_length_prefix_and_the_nul() {
        let m = parse_message("string[] name\n").unwrap();
        let r = caps("[fields]\n\"p/M.name\" = { cap = 2, element_cap = 32 }\n");
        assert_eq!(
            bound_message("p/M", &m, EncodingVersion::Xcdr1, &r, &no_lookup),
            TypeBound::Bounded(4 + 4 + 40 + 37)
        );
    }

    /// Both walks again, for the element dimension. `SchemaCaps` reads the
    /// config for the emitted `Message::FIELDS`; this module reads it for the C
    /// constant. An element bound honoured by one only is the same defect
    /// `a_cap_reaches_the_c_bound_and_the_rust_schema_alike` pins for `cap`.
    #[test]
    fn an_element_cap_reaches_the_c_bound_and_the_rust_schema_alike() {
        use crate::generator::common::{SchemaCaps, build_nros_message_schema};

        let m = parse_message("string[] name\n").unwrap();
        let r = caps("[fields]\n\"p/M.name\" = { cap = 16, element_cap = 32 }\n");

        let built = build_schema("p/M", &m, &r, &no_lookup).unwrap();
        use nros_serdes::schema::FieldType as S;
        match built[0].ty {
            S::BoundedSequence(16, elem) => assert!(matches!(elem, S::BoundedString(32))),
            ref other => panic!("expected BoundedSequence(16, BoundedString(32)), got {other:?}"),
        }

        let schema = build_nros_message_schema("p", "M", &m.fields, &SchemaCaps::new("M", &r));
        assert!(
            schema
                .fields_block
                .contains("FieldType::BoundedSequence(16, &FT_NAME_ELEM)"),
            "{}",
            schema.fields_block
        );
        assert!(
            schema
                .helper_consts
                .contains("FT_NAME_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::BoundedString(32)"),
            "{}",
            schema.helper_consts
        );
    }

    /// Issue 0939 -- `size_bound` walks a bounded sequence ELEMENT BY ELEMENT,
    /// so nested bounded sequences cost the PRODUCT of their caps. W7 must not
    /// make that easier to hit, and this pins WHY it does not.
    ///
    /// A string is a LEAF: `element_cap` can only turn `String` into
    /// `BoundedString`, which has no elements of its own. So the key adds a
    /// LINEAR factor to one level (`cap * (4 + element_cap + 1)`) and cannot add
    /// a level to a bounded-sequence chain -- the depth of the nesting, which is
    /// what the product is over, is a property of the `.msg` alone.
    ///
    /// Checked by construction: the deepest chain of `BoundedSequence` in a
    /// schema is the same with and without the element bound, while the number
    /// grows only in proportion to the cap.
    #[test]
    fn an_element_cap_cannot_deepen_a_bounded_sequence_chain() {
        /// Longest chain of nested `BoundedSequence` reachable from `fields`.
        fn depth(ty: &nros_serdes::schema::FieldType) -> usize {
            use nros_serdes::schema::FieldType as S;
            match ty {
                S::BoundedSequence(_, inner) => 1 + depth(inner),
                S::Array(_, inner) => depth(inner),
                S::Nested(n) => n.fields.iter().map(|f| depth(&f.ty)).max().unwrap_or(0),
                _ => 0,
            }
        }

        let m = parse_message("string[] name\n").unwrap();
        let without = build_schema(
            "p/M",
            &m,
            &caps("[fields]\n\"p/M.name\" = 16\n"),
            &no_lookup,
        )
        .unwrap();
        let with = build_schema(
            "p/M",
            &m,
            &caps("[fields]\n\"p/M.name\" = { cap = 16, element_cap = 32 }\n"),
            &no_lookup,
        )
        .unwrap();
        assert_eq!(depth(&without[0].ty), 1);
        assert_eq!(
            depth(&with[0].ty),
            1,
            "an element bound turns a leaf into a bounded leaf, never into a level"
        );

        // And the growth is linear in the cap, not multiplicative: doubling the
        // sequence cap doubles the element block, it does not square it.
        let one = bound_message(
            "p/M",
            &m,
            EncodingVersion::Xcdr1,
            &caps("[fields]\n\"p/M.name\" = { cap = 1, element_cap = 32 }\n"),
            &no_lookup,
        );
        let two = bound_message(
            "p/M",
            &m,
            EncodingVersion::Xcdr1,
            &caps("[fields]\n\"p/M.name\" = { cap = 2, element_cap = 32 }\n"),
            &no_lookup,
        );
        assert_eq!(one, TypeBound::Bounded(45));
        assert_eq!(two, TypeBound::Bounded(85), "45 + 40, not 45 * 45");
    }
}
