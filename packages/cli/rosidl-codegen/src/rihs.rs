//! phase-304 W1 (RFC-0056) — REP-2011 RIHS01 type-hash engine.
//!
//! The RIHS01 hash is `SHA-256` of the **canonical type description** (NOT the
//! `.msg` text), formatted `RIHS01_<64 lowercase hex>`. Iron+ uses it in the
//! zenoh data keyexpr + liveliness tokens; Humble uses the
//! `TypeHashNotSupported` placeholder. Distilled from `ros2/rcl`
//! (`type_hash.c`) + `rosidl_runtime_c` — see
//! `docs/research/rep-2011-type-hash.md` for the derivation.
//!
//! This module is the pure ENGINE: given a [`TypeDescription`] (the DAG-closed,
//! type-id-mapped description), it produces the exact "hashable JSON" and the
//! `RIHS01_…` string. Building the [`TypeDescription`] from the parsed
//! `.msg`/`.srv` AST + wiring it into codegen is W1b — kept separate so the
//! byte-exact canonical form is testable against the documented reference
//! (`std_msgs/msg/Int32`) with no parser dependency.

use sha2::{Digest, Sha256};

/// REP-2011 `rosidl_runtime_c__type_description__FieldType` `type_id` values
/// (emitted as DECIMAL in the hashed text). Scalars 1..=18; the array /
/// sequence variants are a scalar id plus a fixed offset.
pub mod type_id {
    pub const NESTED_TYPE: u8 = 1;
    pub const INT8: u8 = 2;
    pub const UINT8: u8 = 3;
    pub const INT16: u8 = 4;
    pub const UINT16: u8 = 5;
    pub const INT32: u8 = 6;
    pub const UINT32: u8 = 7;
    pub const INT64: u8 = 8;
    pub const UINT64: u8 = 9;
    pub const FLOAT: u8 = 10;
    pub const DOUBLE: u8 = 11;
    pub const LONG_DOUBLE: u8 = 12;
    pub const CHAR: u8 = 13;
    pub const WCHAR: u8 = 14;
    pub const BOOLEAN: u8 = 15;
    pub const BYTE: u8 = 16;
    pub const STRING: u8 = 17;
    pub const WSTRING: u8 = 18;
    pub const BOUNDED_STRING: u8 = 21;

    /// `scalar_id + 48` → fixed ARRAY of that scalar.
    pub const ARRAY_OFFSET: u8 = 48;
    /// `scalar_id + 96` → BOUNDED_SEQUENCE of that scalar.
    pub const BOUNDED_SEQUENCE_OFFSET: u8 = 96;
    /// `scalar_id + 144` → UNBOUNDED_SEQUENCE of that scalar.
    pub const UNBOUNDED_SEQUENCE_OFFSET: u8 = 144;
}

/// One field's type. Mirrors `rosidl_runtime_c__type_description__FieldType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTypeDesc {
    /// Numeric REP-2011 type id (see [`type_id`]).
    pub type_id: u8,
    /// Array length or sequence upper bound; `0` for a scalar / unbounded.
    pub capacity: u64,
    /// String upper bound; `0` unless a bounded string.
    pub string_capacity: u64,
    /// FQ name for a nested reference (`type_id == NESTED_TYPE` or a
    /// nested-array/sequence); `""` otherwise.
    pub nested_type_name: String,
}

impl FieldTypeDesc {
    /// A plain scalar (no capacity, no nested name).
    pub fn scalar(type_id: u8) -> Self {
        Self {
            type_id,
            capacity: 0,
            string_capacity: 0,
            nested_type_name: String::new(),
        }
    }
}

/// One field of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDesc {
    pub name: String,
    pub ty: FieldTypeDesc,
}

/// A single type's description — `.msg` field order is PRESERVED (never sorted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndividualTypeDescription {
    pub type_name: String,
    pub fields: Vec<FieldDesc>,
}

/// The full, DAG-closed description hashed by RIHS01.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDescription {
    pub type_description: IndividualTypeDescription,
    /// Every transitively-referenced type — the caller supplies them; this
    /// engine sorts them alphabetically by `type_name` (the ONLY sort).
    pub referenced_type_descriptions: Vec<IndividualTypeDescription>,
}

/// JSON-escape a string per the libyaml/JSON rules RIHS uses (double-quoted,
/// with `\"` and `\\` and control-char escapes). Type/field names are plain
/// identifiers in practice, but escape defensively.
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // `ensure_ascii=True`: control chars AND every non-ASCII char are
            // `\uXXXX`-escaped (Python json.dumps default; matches rosidl).
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let cp = c as u32;
                if cp > 0xffff {
                    // Surrogate pair, as Python emits.
                    let v = cp - 0x10000;
                    out.push_str(&format!(
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (v >> 10),
                        0xdc00 + (v & 0x3ff)
                    ));
                } else {
                    out.push_str(&format!("\\u{cp:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn emit_field_type(ft: &FieldTypeDesc, out: &mut String) {
    // Fixed key order: type_id, capacity, string_capacity, nested_type_name.
    // Separators match Python `json.dumps(separators=(', ', ': '))` — a space
    // after every ':' and ',' (verified against Jazzy's own hash, NOT the
    // compact form the research doc guessed).
    out.push_str("{\"type_id\": ");
    out.push_str(&ft.type_id.to_string());
    out.push_str(", \"capacity\": ");
    out.push_str(&ft.capacity.to_string());
    out.push_str(", \"string_capacity\": ");
    out.push_str(&ft.string_capacity.to_string());
    out.push_str(", \"nested_type_name\": ");
    json_str(&ft.nested_type_name, out);
    out.push('}');
}

fn emit_individual(itd: &IndividualTypeDescription, out: &mut String) {
    // Fixed key order: type_name, fields. `default_value` is REMOVED before
    // hashing (rosidl `calculate_type_hash`), so it never appears here.
    out.push_str("{\"type_name\": ");
    json_str(&itd.type_name, out);
    out.push_str(", \"fields\": [");
    for (i, f) in itd.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        // Fixed key order: name, type.
        out.push_str("{\"name\": ");
        json_str(&f.name, out);
        out.push_str(", \"type\": ");
        emit_field_type(&f.ty, out);
        out.push('}');
    }
    out.push(']');
    out.push('}');
}

/// The canonical "hashable JSON" — the exact UTF-8 buffer RIHS SHA-256s.
/// Byte-identical to Python `json.dumps(hashable_dict, separators=(', ', ': '),
/// ensure_ascii=True, sort_keys=False)` (rosidl `calculate_type_hash`): a space
/// after every `:` and `,`, ASCII-escaped strings, dict-insertion key order.
/// `referenced_type_descriptions` is sorted alphabetically by `type_name` (the
/// only sort — done when the DAG is built / here). Confirmed against a live
/// Jazzy install for `std_msgs/msg/Int32` (phase-304 W1b).
pub fn to_hashable_json(desc: &TypeDescription) -> String {
    let mut refs = desc.referenced_type_descriptions.clone();
    refs.sort_by(|a, b| a.type_name.cmp(&b.type_name));

    let mut out = String::new();
    // Fixed key order: type_description, referenced_type_descriptions.
    out.push_str("{\"type_description\": ");
    emit_individual(&desc.type_description, &mut out);
    out.push_str(", \"referenced_type_descriptions\": [");
    for (i, r) in refs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_individual(r, &mut out);
    }
    out.push(']');
    out.push('}');
    out
}

// =============================================================================
// AST → TypeDescription (W1b): build the DAG-closed description from a parsed
// `.msg`. Nested types are resolved via a caller-supplied `resolve` callback so
// the builder is testable without the codegen package loader.
// =============================================================================

use rosidl_parser::ast::{FieldType as AstFieldType, Message, PrimitiveType};

/// The REP-2011 scalar `type_id` for a primitive.
fn primitive_id(p: &PrimitiveType) -> u8 {
    match p {
        PrimitiveType::Bool => type_id::BOOLEAN,
        PrimitiveType::Byte => type_id::BYTE,
        PrimitiveType::Char => type_id::CHAR,
        PrimitiveType::Int8 => type_id::INT8,
        PrimitiveType::UInt8 => type_id::UINT8,
        PrimitiveType::Int16 => type_id::INT16,
        PrimitiveType::UInt16 => type_id::UINT16,
        PrimitiveType::Int32 => type_id::INT32,
        PrimitiveType::UInt32 => type_id::UINT32,
        PrimitiveType::Int64 => type_id::INT64,
        PrimitiveType::UInt64 => type_id::UINT64,
        PrimitiveType::Float32 => type_id::FLOAT,
        PrimitiveType::Float64 => type_id::DOUBLE,
    }
}

/// Fully-qualified `pkg/msg/Name` for a namespaced element. `same_pkg` supplies
/// the current message's package when the reference omits it (a same-package
/// `Name` field).
fn namespaced_fqn(package: &Option<String>, name: &str, same_pkg: &str) -> String {
    let pkg = package.as_deref().unwrap_or(same_pkg);
    // Already-qualified names (`pkg/msg/Name`) pass through.
    if name.contains('/') {
        name.to_string()
    } else {
        format!("{pkg}/msg/{name}")
    }
}

/// The *element* base for the array/sequence offset scheme: the scalar id, its
/// string capacity, and the nested FQN (empty for non-nested). Bounded strings
/// use the UNBOUNDED string base (17/18) here with the bound carried in
/// `string_capacity` — the `+48/+96/+144` offset applies to the generic string
/// id (per the research doc's `string<=20[] → 161` example). A BARE bounded
/// string field uses the distinct scalar id 21 instead (handled in
/// [`field_type_desc`]).
fn element_base(ft: &AstFieldType, same_pkg: &str) -> (u8, u64, String) {
    match ft {
        AstFieldType::Primitive(p) => (primitive_id(p), 0, String::new()),
        AstFieldType::String => (type_id::STRING, 0, String::new()),
        AstFieldType::BoundedString(n) => (type_id::STRING, *n as u64, String::new()),
        AstFieldType::WString => (type_id::WSTRING, 0, String::new()),
        AstFieldType::BoundedWString(n) => (type_id::WSTRING, *n as u64, String::new()),
        AstFieldType::NamespacedType { package, name } => (
            type_id::NESTED_TYPE,
            0,
            namespaced_fqn(package, name, same_pkg),
        ),
        // Nested collections (array-of-sequence etc.) are not valid ROS `.msg`
        // field types — the parser never produces them; treat defensively as a
        // nested reference to the inner element's base.
        AstFieldType::Array { element_type, .. }
        | AstFieldType::Sequence { element_type }
        | AstFieldType::BoundedSequence { element_type, .. } => {
            element_base(element_type, same_pkg)
        }
    }
}

/// Map one parsed field type to its REP-2011 [`FieldTypeDesc`]. `same_pkg` is
/// the enclosing message's package (for same-package nested refs).
pub fn field_type_desc(ft: &AstFieldType, same_pkg: &str) -> FieldTypeDesc {
    match ft {
        // Bare bounded string uses the distinct scalar id 21 (NOT the 17 base
        // the collection offset uses).
        AstFieldType::BoundedString(n) => FieldTypeDesc {
            type_id: type_id::BOUNDED_STRING,
            capacity: 0,
            string_capacity: *n as u64,
            nested_type_name: String::new(),
        },
        AstFieldType::Array { element_type, size } => {
            let (base, str_cap, nested) = element_base(element_type, same_pkg);
            FieldTypeDesc {
                type_id: base + type_id::ARRAY_OFFSET,
                capacity: *size as u64,
                string_capacity: str_cap,
                nested_type_name: nested,
            }
        }
        AstFieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let (base, str_cap, nested) = element_base(element_type, same_pkg);
            FieldTypeDesc {
                type_id: base + type_id::BOUNDED_SEQUENCE_OFFSET,
                capacity: *max_size as u64,
                string_capacity: str_cap,
                nested_type_name: nested,
            }
        }
        AstFieldType::Sequence { element_type } => {
            let (base, str_cap, nested) = element_base(element_type, same_pkg);
            FieldTypeDesc {
                type_id: base + type_id::UNBOUNDED_SEQUENCE_OFFSET,
                capacity: 0,
                string_capacity: str_cap,
                nested_type_name: nested,
            }
        }
        // Scalar (non-collection) cases via the shared element base.
        _ => {
            let (id, str_cap, nested) = element_base(ft, same_pkg);
            FieldTypeDesc {
                type_id: id,
                capacity: 0,
                string_capacity: str_cap,
                nested_type_name: nested,
            }
        }
    }
}

/// One message's [`IndividualTypeDescription`] (constants are NOT part of the
/// type description — only fields, in source order).
pub fn message_to_individual(type_name: &str, msg: &Message) -> IndividualTypeDescription {
    let same_pkg = type_name.split('/').next().unwrap_or("");
    IndividualTypeDescription {
        type_name: type_name.to_string(),
        fields: msg
            .fields
            .iter()
            .map(|f| FieldDesc {
                name: f.name.clone(),
                ty: field_type_desc(&f.field_type, same_pkg),
            })
            .collect(),
    }
}

/// Collect the FQNs of every nested type a message references (`pkg/msg/Name`).
fn nested_refs(itd: &IndividualTypeDescription) -> Vec<String> {
    itd.fields
        .iter()
        .filter(|f| !f.ty.nested_type_name.is_empty())
        .map(|f| f.ty.nested_type_name.clone())
        .collect()
}

/// Build the DAG-closed [`TypeDescription`] for `type_name` + its parsed
/// `Message`, resolving every transitively-referenced type via `resolve`
/// (`fqn → Message`). Referenced types are de-duplicated; the engine sorts them
/// at hash time. `resolve` returning `None` for a referenced type is an error
/// (the description would be incomplete → a wrong hash).
pub fn build_type_description(
    type_name: &str,
    msg: &Message,
    resolve: impl Fn(&str) -> Option<Message>,
) -> Result<TypeDescription, String> {
    let top = message_to_individual(type_name, msg);

    // BFS over nested refs, keeping insertion order stable (sort happens at
    // hash time); a `seen` set de-dups the DAG.
    let mut referenced: Vec<IndividualTypeDescription> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    seen.insert(type_name.to_string()); // self-ref is never repeated in referenced
    let mut queue: std::collections::VecDeque<String> = nested_refs(&top).into_iter().collect();

    while let Some(fqn) = queue.pop_front() {
        if !seen.insert(fqn.clone()) {
            continue;
        }
        let nested_msg = resolve(&fqn).ok_or_else(|| {
            format!("RIHS: cannot resolve nested type '{fqn}' referenced from '{type_name}'")
        })?;
        let itd = message_to_individual(&fqn, &nested_msg);
        for r in nested_refs(&itd) {
            if !seen.contains(&r) {
                queue.push_back(r);
            }
        }
        referenced.push(itd);
    }

    Ok(TypeDescription {
        type_description: top,
        referenced_type_descriptions: referenced,
    })
}

/// Compute the `RIHS01_<64 hex>` type hash of a canonical [`TypeDescription`].
pub fn rihs01(desc: &TypeDescription) -> String {
    let json = to_hashable_json(desc);
    let digest = Sha256::digest(json.as_bytes());
    let mut s = String::with_capacity(71);
    s.push_str("RIHS01_");
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented `std_msgs/msg/Int32` canonical JSON (research doc §3):
    /// `.msg` is a single `int32 data` field, no referenced types.
    #[test]
    fn int32_canonical_json_matches_the_documented_reference() {
        let desc = TypeDescription {
            type_description: IndividualTypeDescription {
                type_name: "std_msgs/msg/Int32".to_string(),
                fields: vec![FieldDesc {
                    name: "data".to_string(),
                    ty: FieldTypeDesc::scalar(type_id::INT32),
                }],
            },
            referenced_type_descriptions: vec![],
        };
        // Byte-identical to Jazzy's `json.dumps(separators=(', ', ': '))` form
        // (a space after every ':' and ','); default_value removed.
        let expected = "{\"type_description\": {\"type_name\": \"std_msgs/msg/Int32\", \"fields\": [{\"name\": \"data\", \"type\": {\"type_id\": 6, \"capacity\": 0, \"string_capacity\": 0, \"nested_type_name\": \"\"}}]}, \"referenced_type_descriptions\": []}";
        assert_eq!(to_hashable_json(&desc), expected);
    }

    #[test]
    fn rihs01_has_the_right_shape_and_is_deterministic() {
        let desc = TypeDescription {
            type_description: IndividualTypeDescription {
                type_name: "std_msgs/msg/Int32".to_string(),
                fields: vec![FieldDesc {
                    name: "data".to_string(),
                    ty: FieldTypeDesc::scalar(type_id::INT32),
                }],
            },
            referenced_type_descriptions: vec![],
        };
        let h = rihs01(&desc);
        assert!(h.starts_with("RIHS01_"));
        assert_eq!(h.len(), 71);
        assert!(
            h[7..]
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        // Deterministic: same input → same hash.
        assert_eq!(h, rihs01(&desc));
        // CONFIRMED against a live ROS 2 Jazzy install (phase-304 W1b, via
        // `docker run ros:jazzy-ros-base cat
        // /opt/ros/jazzy/share/std_msgs/msg/Int32.json` → `type_hashes[0]`).
        // This is the REAL REP-2011 hash, not just a deterministic snapshot — a
        // diff here is a genuine wire-break.
        assert_eq!(
            h,
            "RIHS01_b6578ded3c58c626cfe8d1a6fb6e04f706f97e9f03d2727c9ff4e74b1cef0deb"
        );
    }

    /// Referenced types are sorted alphabetically by `type_name`; fields keep
    /// source order.
    #[test]
    fn referenced_types_sort_alphabetically_fields_do_not() {
        let mk = |n: &str| IndividualTypeDescription {
            type_name: n.to_string(),
            fields: vec![],
        };
        let desc = TypeDescription {
            type_description: IndividualTypeDescription {
                type_name: "p/msg/Top".to_string(),
                // Fields in a deliberately non-alphabetical order — must be kept.
                fields: vec![
                    FieldDesc {
                        name: "zzz".to_string(),
                        ty: FieldTypeDesc::scalar(type_id::UINT8),
                    },
                    FieldDesc {
                        name: "aaa".to_string(),
                        ty: FieldTypeDesc::scalar(type_id::UINT8),
                    },
                ],
            },
            referenced_type_descriptions: vec![mk("b/msg/B"), mk("a/msg/A")],
        };
        let json = to_hashable_json(&desc);
        // referenced sorted: a/msg/A before b/msg/B.
        let a = json.find("a/msg/A").unwrap();
        let b = json.find("b/msg/B").unwrap();
        assert!(a < b, "referenced types must be alphabetical: {json}");
        // fields NOT sorted: zzz before aaa (source order).
        let z = json.find("zzz").unwrap();
        let aa = json.find("aaa").unwrap();
        assert!(z < aa, "fields must keep source order: {json}");
    }

    // --- W1b: AST → TypeDescription ---
    use rosidl_parser::ast::{Field, FieldType as Ast, Message, PrimitiveType};

    fn field(name: &str, ft: Ast) -> Field {
        Field {
            field_type: ft,
            name: name.to_string(),
            default_value: None,
        }
    }

    #[test]
    fn field_type_desc_maps_primitives_arrays_sequences_strings() {
        // int32 → scalar id 6.
        assert_eq!(
            field_type_desc(&Ast::Primitive(PrimitiveType::Int32), "p"),
            FieldTypeDesc::scalar(type_id::INT32)
        );
        // int32[4] → 6+48 = 54, capacity 4.
        assert_eq!(
            field_type_desc(
                &Ast::Array {
                    element_type: Box::new(Ast::Primitive(PrimitiveType::Int32)),
                    size: 4
                },
                "p"
            ),
            FieldTypeDesc {
                type_id: 54,
                capacity: 4,
                string_capacity: 0,
                nested_type_name: String::new()
            }
        );
        // string<=20[] → 17+144 = 161, string_capacity 20 (research doc example).
        assert_eq!(
            field_type_desc(
                &Ast::Sequence {
                    element_type: Box::new(Ast::BoundedString(20))
                },
                "p"
            ),
            FieldTypeDesc {
                type_id: 161,
                capacity: 0,
                string_capacity: 20,
                nested_type_name: String::new()
            }
        );
        // bare string<=20 → the distinct scalar id 21.
        assert_eq!(
            field_type_desc(&Ast::BoundedString(20), "p"),
            FieldTypeDesc {
                type_id: type_id::BOUNDED_STRING,
                capacity: 0,
                string_capacity: 20,
                nested_type_name: String::new()
            }
        );
        // a same-package nested ref → NESTED_TYPE + `p/msg/Name`.
        assert_eq!(
            field_type_desc(
                &Ast::NamespacedType {
                    package: None,
                    name: "Vector3".to_string()
                },
                "geometry_msgs"
            ),
            FieldTypeDesc {
                type_id: type_id::NESTED_TYPE,
                capacity: 0,
                string_capacity: 0,
                nested_type_name: "geometry_msgs/msg/Vector3".to_string()
            }
        );
    }

    #[test]
    fn build_dag_closes_over_nested_types_via_resolver() {
        // Top: a message with one nested `Inner` field + a scalar.
        let top = Message {
            fields: vec![
                field(
                    "inner",
                    Ast::NamespacedType {
                        package: Some("pkg".to_string()),
                        name: "Inner".to_string(),
                    },
                ),
                field("n", Ast::Primitive(PrimitiveType::UInt32)),
            ],
            constants: vec![],
        };
        // Inner: a leaf message (one field, no further nesting).
        let inner = Message {
            fields: vec![field("x", Ast::Primitive(PrimitiveType::Float64))],
            constants: vec![],
        };
        let resolve = |fqn: &str| (fqn == "pkg/msg/Inner").then(|| inner.clone());

        let desc = build_type_description("pkg/msg/Top", &top, resolve).unwrap();
        // Inner is in referenced_type_descriptions; Top's `inner` field is NESTED.
        assert_eq!(desc.referenced_type_descriptions.len(), 1);
        assert_eq!(
            desc.referenced_type_descriptions[0].type_name,
            "pkg/msg/Inner"
        );
        assert_eq!(
            desc.type_description.fields[0].ty.type_id,
            type_id::NESTED_TYPE
        );
        assert_eq!(
            desc.type_description.fields[0].ty.nested_type_name,
            "pkg/msg/Inner"
        );
        // The canonical JSON names both types + is deterministic.
        let json = to_hashable_json(&desc);
        assert!(
            json.contains("pkg/msg/Top") && json.contains("pkg/msg/Inner"),
            "{json}"
        );
        assert!(rihs01(&desc).starts_with("RIHS01_"));
    }

    /// End-to-end against a LIVE Jazzy reference: `std_msgs/msg/Header`
    /// (nested `builtin_interfaces/msg/Time` + a `string frame_id`). Both the
    /// Header hash and the referenced Time hash are the real values from
    /// `/opt/ros/jazzy/share/{std_msgs,builtin_interfaces}/msg/*.json`
    /// (phase-304 W1b). Confirms the DAG closure + NESTED_TYPE + STRING mapping.
    #[test]
    fn header_with_nested_time_matches_live_jazzy() {
        let time = Message {
            fields: vec![
                field("sec", Ast::Primitive(PrimitiveType::Int32)),
                field("nanosec", Ast::Primitive(PrimitiveType::UInt32)),
            ],
            constants: vec![],
        };
        let header = Message {
            fields: vec![
                field(
                    "stamp",
                    Ast::NamespacedType {
                        package: Some("builtin_interfaces".to_string()),
                        name: "Time".to_string(),
                    },
                ),
                field("frame_id", Ast::String),
            ],
            constants: vec![],
        };
        let resolve = |fqn: &str| (fqn == "builtin_interfaces/msg/Time").then(|| time.clone());

        // The referenced Time on its own hashes to Jazzy's Time hash.
        let time_desc =
            build_type_description("builtin_interfaces/msg/Time", &time, |_| None).unwrap();
        assert_eq!(
            rihs01(&time_desc),
            "RIHS01_b106235e25a4c5ed35098aa0a61a3ee9c9b18d197f398b0e4206cea9acf9c197"
        );
        // Header (Time in referenced) hashes to Jazzy's Header hash.
        let header_desc = build_type_description("std_msgs/msg/Header", &header, resolve).unwrap();
        assert_eq!(
            rihs01(&header_desc),
            "RIHS01_f49fb3ae2cf070f793645ff749683ac6b06203e41c891e17701b1cb597ce6a01"
        );
    }

    #[test]
    fn build_dag_errors_loud_on_unresolvable_nested_type() {
        let top = Message {
            fields: vec![field(
                "inner",
                Ast::NamespacedType {
                    package: Some("pkg".to_string()),
                    name: "Missing".to_string(),
                },
            )],
            constants: vec![],
        };
        let err = build_type_description("pkg/msg/Top", &top, |_| None).unwrap_err();
        assert!(err.contains("cannot resolve nested type"), "got: {err}");
    }
}
