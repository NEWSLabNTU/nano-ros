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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn emit_field_type(ft: &FieldTypeDesc, out: &mut String) {
    // Fixed key order: type_id, capacity, string_capacity, nested_type_name.
    out.push_str("{\"type_id\":");
    out.push_str(&ft.type_id.to_string());
    out.push_str(",\"capacity\":");
    out.push_str(&ft.capacity.to_string());
    out.push_str(",\"string_capacity\":");
    out.push_str(&ft.string_capacity.to_string());
    out.push_str(",\"nested_type_name\":");
    json_str(&ft.nested_type_name, out);
    out.push('}');
}

fn emit_individual(itd: &IndividualTypeDescription, out: &mut String) {
    // Fixed key order: type_name, fields.
    out.push_str("{\"type_name\":");
    json_str(&itd.type_name, out);
    out.push_str(",\"fields\":[");
    for (i, f) in itd.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // Fixed key order: name, type.
        out.push_str("{\"name\":");
        json_str(&f.name, out);
        out.push_str(",\"type\":");
        emit_field_type(&f.ty, out);
        out.push('}');
    }
    out.push_str("]}");
}

/// The canonical "hashable JSON" — the exact UTF-8 buffer RIHS SHA-256s.
/// libyaml flow style: no whitespace, double-quoted keys/strings, plain
/// numerics, one line. Fixed key order per REP-2011 (§2 of the research doc).
/// `referenced_type_descriptions` is sorted alphabetically by `type_name` here.
pub fn to_hashable_json(desc: &TypeDescription) -> String {
    let mut refs = desc.referenced_type_descriptions.clone();
    refs.sort_by(|a, b| a.type_name.cmp(&b.type_name));

    let mut out = String::new();
    // Fixed key order: type_description, referenced_type_descriptions.
    out.push_str("{\"type_description\":");
    emit_individual(&desc.type_description, &mut out);
    out.push_str(",\"referenced_type_descriptions\":[");
    for (i, r) in refs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        emit_individual(r, &mut out);
    }
    out.push_str("]}");
    out
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
        let expected = "{\"type_description\":{\"type_name\":\"std_msgs/msg/Int32\",\"fields\":[{\"name\":\"data\",\"type\":{\"type_id\":6,\"capacity\":0,\"string_capacity\":0,\"nested_type_name\":\"\"}}]},\"referenced_type_descriptions\":[]}";
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
        // REGRESSION SNAPSHOT of THIS engine's output for the documented Int32
        // canonical JSON. It is NOT yet confirmed to equal the real REP-2011
        // value — phase-304 W4's capture script cross-checks it against a Jazzy
        // `ros2 interface hash std_msgs/msg/Int32`. If W4 finds a mismatch, the
        // canonical form (not this assertion) has the bug. A change to this
        // snapshot without a W4 confirmation is a silent wire-break — treat a
        // diff here as load-bearing.
        assert_eq!(
            h,
            "RIHS01_22ff2de7c2a194b0515c3169c17368e86ab95adbcdb2b6e6e05d5f5e011f99b6"
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
}
