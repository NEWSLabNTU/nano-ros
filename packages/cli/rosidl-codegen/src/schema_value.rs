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
