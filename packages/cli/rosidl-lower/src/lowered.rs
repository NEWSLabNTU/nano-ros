//! The target-concrete, language-neutral IR (RFC-0068 Stage 2 output).
//!
//! `lower()` takes a [`ResolvedMessage`], the capacity [`CapacityResolver`]
//! (CodegenConfig) and a [`TargetProfile`], and computes the per-field facts a
//! renderer must not re-derive: storage decision, plainness, alignment, the CDR
//! op, and the field order. Language spelling is NOT here — a template maps the
//! neutral facts to `u32`/`uint32_t`/`write_u32`/… (RFC-0068 Stage 3).

use rosidl_parser::ast::{FieldType, PrimitiveType};
use rosidl_resolve::ResolvedMessage;

use crate::config::{CapacityResolver, FieldKind, FieldStorage, StorageMode};

/// A build target's layout parameters. CDR scalar sizes are wire-fixed and do
/// not vary by target; what varies is pointer width and how a `repr(C)` enum is
/// sized (ARM EABI short-enums pack an enum to 1 byte where x86_64 uses 4 —
/// project memory / RFC-0054). Carried so a layout fact is computed for the
/// target being built, never assumed host-64-bit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfile {
    pub name: String,
    /// Pointer width in bytes (4 or 8).
    pub ptr_width: usize,
    /// `repr(C)` enum width in bytes (1 under short-enums, else 4).
    pub enum_width: usize,
}

impl TargetProfile {
    /// The host development target (x86_64 / aarch64 hosted): 8-byte pointers,
    /// 4-byte enums.
    pub fn host() -> Self {
        Self {
            name: "host".into(),
            ptr_width: 8,
            enum_width: 4,
        }
    }

    /// An ARM EABI bare-metal target: 4-byte pointers, 1-byte short-enums.
    pub fn arm_eabi() -> Self {
        Self {
            name: "arm-eabi".into(),
            ptr_width: 4,
            enum_width: 1,
        }
    }
}

/// Neutral CDR read/write op for a scalar. A renderer maps this to its own
/// method name (`write_u32` / `z_serialize_uint32` / …) — the op itself is
/// language-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdrOp {
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    /// A string member (length-prefixed).
    String,
    /// A nested message member (delegates to its own (de)serialize).
    Nested,
}

impl CdrOp {
    fn from_primitive(p: PrimitiveType) -> Self {
        match p {
            PrimitiveType::Bool => CdrOp::Bool,
            PrimitiveType::Byte | PrimitiveType::UInt8 => CdrOp::U8,
            PrimitiveType::Char => CdrOp::U8,
            PrimitiveType::Int8 => CdrOp::I8,
            PrimitiveType::UInt16 => CdrOp::U16,
            PrimitiveType::Int16 => CdrOp::I16,
            PrimitiveType::UInt32 => CdrOp::U32,
            PrimitiveType::Int32 => CdrOp::I32,
            PrimitiveType::UInt64 => CdrOp::U64,
            PrimitiveType::Int64 => CdrOp::I64,
            PrimitiveType::Float32 => CdrOp::F32,
            PrimitiveType::Float64 => CdrOp::F64,
        }
    }

    /// CDR wire size (bytes) of the scalar, which equals its natural alignment.
    fn cdr_size(self) -> usize {
        match self {
            CdrOp::Bool | CdrOp::U8 | CdrOp::I8 => 1,
            CdrOp::U16 | CdrOp::I16 => 2,
            CdrOp::U32 | CdrOp::I32 | CdrOp::F32 => 4,
            CdrOp::U64 | CdrOp::I64 | CdrOp::F64 => 8,
            // String / Nested are not fixed-size scalars.
            CdrOp::String | CdrOp::Nested => 0,
        }
    }

    /// Whether the scalar can participate in a POD blit fast path — the numeric
    /// integer/float ops only. `bool` is excluded (CDR bool is a constrained
    /// `u8`, not an arbitrary byte), as are the non-scalar `String`/`Nested`.
    fn is_plain_scalar(self) -> bool {
        matches!(
            self,
            CdrOp::U8
                | CdrOp::I8
                | CdrOp::U16
                | CdrOp::I16
                | CdrOp::U32
                | CdrOp::I32
                | CdrOp::U64
                | CdrOp::I64
                | CdrOp::F32
                | CdrOp::F64
        )
    }
}

/// How a field's payload is stored in the generated struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStorage {
    /// Value stored inline (scalars, fixed arrays, single nested struct).
    Inline,
    /// Fixed-capacity buffer of `cap` (bounded string, or an unbounded string
    /// the config pins to inline).
    Fixed { cap: usize },
    /// Bounded sequence of `cap` (`type[<=N]`, or an unbounded sequence the
    /// config pins to inline).
    Bounded { cap: usize },
    /// Heap-backed (`alloc::Vec` / `String` — the config's `heap` mode).
    Heap,
    /// Zero-copy borrow of `cap` into the CDR receive buffer (the config's
    /// `borrowed` mode — RFC-0033 / issue 0007).
    Borrowed { cap: usize },
}

impl LoweredStorage {
    /// Recover the `(mode, cap)` a `CapacityResolver::resolve` would have
    /// produced for a configurable (unbounded string / sequence) field. Lets a
    /// codegen builder read the storage decision from the IR instead of calling
    /// the resolver a second time (phase-335 W1.c). `Inline` is not a
    /// configurable-storage field and maps to `(Owned, 0)` defensively.
    pub fn as_field_storage(&self) -> FieldStorage {
        match *self {
            LoweredStorage::Inline => FieldStorage {
                cap: 0,
                mode: StorageMode::Inline,
            },
            LoweredStorage::Fixed { cap } | LoweredStorage::Bounded { cap } => FieldStorage {
                cap,
                mode: StorageMode::Inline,
            },
            LoweredStorage::Heap => FieldStorage {
                cap: 0,
                mode: StorageMode::Heap,
            },
            LoweredStorage::Borrowed { cap } => FieldStorage {
                cap,
                mode: StorageMode::View,
            },
        }
    }
}

/// The shape class of a field — what the renderer branches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldShape {
    Scalar,
    /// Length-prefixed string (single).
    Str,
    /// Fixed-size array `type[N]`.
    Array {
        len: usize,
    },
    /// Sequence `type[]` / `type[<=N]`.
    Sequence,
    /// Single nested message.
    Nested,
}

/// One field lowered to concrete, target-specific, language-neutral facts.
#[derive(Debug, Clone)]
pub struct LoweredField {
    pub name: String,
    /// The original parsed type (the renderer still needs it for spelling).
    pub field_type: FieldType,
    pub shape: FieldShape,
    pub storage: LoweredStorage,
    /// CDR op of the field's scalar (or its element, for arrays/sequences);
    /// `None` for a single nested struct.
    pub cdr_op: Option<CdrOp>,
    /// Alignment of the field's payload, bytes.
    pub align: usize,
    /// Whether this field is POD-blit eligible.
    pub plain: bool,
}

/// A message lowered to the target-concrete IR.
#[derive(Debug, Clone)]
pub struct LoweredType {
    pub type_name: String,
    pub type_hash: String,
    /// Fields in `repr(C)` order. ROS/CDR is declaration-order-positional, so
    /// this is the parsed order — carried explicitly so the fact is a fact, not
    /// an assumption a renderer re-derives.
    pub fields: Vec<LoweredField>,
    /// Struct alignment = max field alignment (min 1).
    pub align: usize,
    /// POD-blit eligible: every field plain AND all fields share one alignment
    /// (else `repr(C)` inserts inter-field padding and the blit is unsound).
    pub plain: bool,
    /// The target this was lowered for.
    pub target: TargetProfile,
}

/// Lower a resolved message for `target` under the capacity `config`.
pub fn lower(
    resolved: &ResolvedMessage,
    config: &CapacityResolver,
    target: &TargetProfile,
) -> LoweredType {
    // `pkg/msg/Name` → (pkg, Name) for the config lookup keys.
    let (package, message) = split_type_name(&resolved.type_name);
    let fields = lower_fields(package, message, &resolved.parsed.fields, config, target);

    let align = fields.iter().map(|f| f.align).max().unwrap_or(1).max(1);
    // Plain iff every field is plain AND all fields share one alignment (uniform
    // alignment ⇒ no inter-field or trailing padding under repr(C)).
    let uniform_align = fields
        .iter()
        .map(|f| f.align)
        .collect::<std::collections::BTreeSet<_>>();
    let plain = !fields.is_empty() && fields.iter().all(|f| f.plain) && uniform_align.len() == 1;

    LoweredType {
        type_name: resolved.type_name.clone(),
        type_hash: resolved.type_hash.clone(),
        fields,
        align,
        plain,
        target: target.clone(),
    }
}

/// Lower every field of `msg` (named `package` / `message` for the config
/// lookup keys) to its concrete facts. Exposed so a codegen builder can read
/// per-field storage from the IR rather than re-resolving it (phase-335 W1.c).
pub fn lower_fields(
    package: &str,
    message: &str,
    fields: &[rosidl_parser::ast::Field],
    config: &CapacityResolver,
    target: &TargetProfile,
) -> Vec<LoweredField> {
    fields
        .iter()
        .map(|f| lower_field(&f.name, &f.field_type, package, message, config, target))
        .collect()
}

fn lower_field(
    name: &str,
    ft: &FieldType,
    package: &str,
    message: &str,
    config: &CapacityResolver,
    target: &TargetProfile,
) -> LoweredField {
    let (shape, storage, cdr_op, align, plain) = match ft {
        FieldType::Primitive(p) => {
            let op = CdrOp::from_primitive(*p);
            (
                FieldShape::Scalar,
                LoweredStorage::Inline,
                Some(op),
                op.cdr_size().max(1),
                op.is_plain_scalar(),
            )
        }
        FieldType::String | FieldType::WString => {
            let s = config.resolve(package, message, name, FieldKind::String);
            let storage = match s.mode {
                StorageMode::Inline => LoweredStorage::Fixed { cap: s.cap },
                StorageMode::Heap => LoweredStorage::Heap,
                StorageMode::View => LoweredStorage::Borrowed { cap: s.cap },
            };
            (FieldShape::Str, storage, Some(CdrOp::String), 4, false)
        }
        FieldType::BoundedString(n) | FieldType::BoundedWString(n) => (
            FieldShape::Str,
            LoweredStorage::Fixed { cap: *n },
            Some(CdrOp::String),
            4,
            false,
        ),
        FieldType::Array { element_type, size } => {
            let (op, elem_align, elem_plain) = element_facts(element_type, target);
            (
                FieldShape::Array { len: *size },
                LoweredStorage::Inline,
                op,
                elem_align,
                // A fixed array of a plain element is itself plain.
                elem_plain,
            )
        }
        FieldType::Sequence { element_type } => {
            let (op, elem_align, _) = element_facts(element_type, target);
            let s = config.resolve(package, message, name, FieldKind::Sequence);
            let storage = match s.mode {
                StorageMode::Inline => LoweredStorage::Bounded { cap: s.cap },
                StorageMode::Heap => LoweredStorage::Heap,
                StorageMode::View => LoweredStorage::Borrowed { cap: s.cap },
            };
            (FieldShape::Sequence, storage, op, elem_align.max(4), false)
        }
        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let (op, elem_align, _) = element_facts(element_type, target);
            (
                FieldShape::Sequence,
                LoweredStorage::Bounded { cap: *max_size },
                op,
                elem_align.max(4),
                false,
            )
        }
        FieldType::NamespacedType { .. } => (
            FieldShape::Nested,
            LoweredStorage::Inline,
            None,
            // A nested struct's alignment is unknown without its own lowering;
            // use pointer width as a conservative stand-in (it never makes a
            // plain struct — nested fields set plain=false).
            target.ptr_width,
            false,
        ),
    };

    LoweredField {
        name: name.to_string(),
        field_type: ft.clone(),
        shape,
        storage,
        cdr_op,
        align,
        plain,
    }
}

/// Facts about an array/sequence element: its CDR op (None if nested), its
/// alignment, and whether it is plain.
fn element_facts(elem: &FieldType, target: &TargetProfile) -> (Option<CdrOp>, usize, bool) {
    match elem {
        FieldType::Primitive(p) => {
            let op = CdrOp::from_primitive(*p);
            (Some(op), op.cdr_size().max(1), op.is_plain_scalar())
        }
        FieldType::String
        | FieldType::WString
        | FieldType::BoundedString(_)
        | FieldType::BoundedWString(_) => (Some(CdrOp::String), 4, false),
        FieldType::NamespacedType { .. } => (None, target.ptr_width, false),
        // Nested arrays/sequences of arrays are not a ROS .msg shape.
        FieldType::Array { element_type, .. }
        | FieldType::Sequence { element_type }
        | FieldType::BoundedSequence { element_type, .. } => element_facts(element_type, target),
    }
}

fn split_type_name(type_name: &str) -> (&str, &str) {
    // `pkg/msg/Name` → ("pkg", "Name"); tolerate `pkg/Name`.
    let mut it = type_name.split('/');
    let pkg = it.next().unwrap_or("");
    let name = type_name.rsplit('/').next().unwrap_or(type_name);
    (pkg, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosidl_parser::parse_message;

    fn no_deps(_: &str) -> Option<rosidl_parser::Message> {
        None
    }

    fn lower_shapes() -> LoweredType {
        // A subset of the fingerprint corpus's Shapes.msg — every field shape.
        let src = "\
bool     flag
uint32   u32_v
float64  f64_v
string   text
int64[]      seq_prim
float64[3]   arr_fixed
int32[<=4]   seq_bounded
string<=8    str_bounded
";
        let msg = parse_message(src).unwrap();
        let r = ResolvedMessage::resolve("shapes_msgs/msg/Shapes", &msg, no_deps).unwrap();
        lower(&r, &CapacityResolver::empty(), &TargetProfile::host())
    }

    fn field<'a>(t: &'a LoweredType, name: &str) -> &'a LoweredField {
        t.fields.iter().find(|f| f.name == name).unwrap()
    }

    #[test]
    fn scalar_facts() {
        let t = lower_shapes();
        let u = field(&t, "u32_v");
        assert_eq!(u.shape, FieldShape::Scalar);
        assert_eq!(u.storage, LoweredStorage::Inline);
        assert_eq!(u.cdr_op, Some(CdrOp::U32));
        assert_eq!(u.align, 4);
        assert!(u.plain);
        // bool is not plain (constrained CDR u8).
        assert!(!field(&t, "flag").plain);
        assert_eq!(field(&t, "f64_v").align, 8);
    }

    #[test]
    fn string_and_bounded_facts() {
        let t = lower_shapes();
        // unbounded string, empty config → the `owned` default: a fixed-capacity
        // buffer sized to the nros string default (256).
        assert_eq!(
            field(&t, "text").storage,
            LoweredStorage::Fixed { cap: 256 }
        );
        assert_eq!(field(&t, "text").shape, FieldShape::Str);
        // bounded string carries its .msg cap.
        assert_eq!(
            field(&t, "str_bounded").storage,
            LoweredStorage::Fixed { cap: 8 }
        );
    }

    #[test]
    fn array_and_sequence_facts() {
        let t = lower_shapes();
        let arr = field(&t, "arr_fixed");
        assert_eq!(arr.shape, FieldShape::Array { len: 3 });
        assert_eq!(arr.storage, LoweredStorage::Inline);
        assert_eq!(arr.cdr_op, Some(CdrOp::F64));
        // unbounded sequence → owned default (bounded to the nros seq default 64);
        // an explicitly bounded sequence carries its .msg cap.
        assert_eq!(
            field(&t, "seq_prim").storage,
            LoweredStorage::Bounded { cap: 64 }
        );
        assert_eq!(
            field(&t, "seq_bounded").storage,
            LoweredStorage::Bounded { cap: 4 }
        );
    }

    #[test]
    fn same_resolved_message_lowers_differently_per_target() {
        // "hash once, lower per target" (RFC-0068): one ResolvedMessage, two
        // TargetProfiles. A nested field's payload alignment follows the
        // target's pointer width — 8 on host, 4 on arm-eabi — while the hash
        // (a Resolve fact) is identical across both lowerings.
        let inner = parse_message("int32 a\n").unwrap();
        let outer = parse_message("test_msgs/Inner child\nint32 tag\n").unwrap();
        let resolve = |fqn: &str| -> Option<rosidl_parser::Message> {
            if fqn.ends_with("/Inner") || fqn == "test_msgs/Inner" {
                Some(inner.clone())
            } else {
                None
            }
        };
        let r = ResolvedMessage::resolve("test_msgs/msg/Outer", &outer, resolve).unwrap();

        let host = lower(&r, &CapacityResolver::empty(), &TargetProfile::host());
        let arm = lower(&r, &CapacityResolver::empty(), &TargetProfile::arm_eabi());

        // Hash is a Resolve fact — same for both targets.
        assert_eq!(host.type_hash, arm.type_hash);
        // The nested field's alignment tracks the target pointer width.
        assert_eq!(field(&host, "child").align, 8);
        assert_eq!(field(&arm, "child").align, 4);
        // Neither is plain (a nested field is never plain).
        assert!(!host.plain && !arm.plain);
    }

    #[test]
    fn struct_not_plain_when_mixed_alignment_or_strings() {
        // Shapes has strings/sequences → not plain.
        assert!(!lower_shapes().plain);

        // A uniform-alignment all-scalar struct IS plain.
        let msg = parse_message("uint32 a\nint32 b\nfloat32 c\n").unwrap();
        let r = ResolvedMessage::resolve("m/msg/AllU32", &msg, no_deps).unwrap();
        let t = lower(&r, &CapacityResolver::empty(), &TargetProfile::host());
        assert!(t.plain, "all-4-byte-scalar struct should be plain");
        assert_eq!(t.align, 4);

        // Mixed alignment (u8 + u32) → padding → not plain.
        let msg2 = parse_message("uint8 a\nuint32 b\n").unwrap();
        let r2 = ResolvedMessage::resolve("m/msg/Mixed", &msg2, no_deps).unwrap();
        let t2 = lower(&r2, &CapacityResolver::empty(), &TargetProfile::host());
        assert!(!t2.plain, "mixed-alignment struct must not be plain");
    }
}
