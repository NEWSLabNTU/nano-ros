//! Static field schema for runtime introspection.
//!
//! Each generated message type exposes its CDR field layout as a
//! `&'static [Field]` slice plus a `&'static str` ROS type name via the
//! [`Message`] trait. Backends that need to construct type descriptors at
//! runtime (Cyclone DDS dynamic types, FastRTPS DynamicTypeBuilder, …) walk
//! this static metadata instead of pulling in per-RMW codegen at compile
//! time.
//!
//! All schema items are `&'static` / [`Copy`] / contain no allocations,
//! keeping the surface usable on `no_std` + alloc-free embedded targets.
//!
//! # Example
//!
//! ```
//! use nros_serdes::schema::{Field, FieldType, Message};
//!
//! /// Hand-rolled mirror of `std_msgs/msg/Int32`.
//! pub struct Int32 {
//!     pub data: i32,
//! }
//!
//! impl Message for Int32 {
//!     const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
//!     const FIELDS: &'static [Field] = &[Field {
//!         name: "data",
//!         ty: FieldType::Int32,
//!         offset: 0,
//!     }];
//! }
//!
//! assert_eq!(Int32::FIELDS.len(), 1);
//! assert!(matches!(Int32::FIELDS[0].ty, FieldType::Int32));
//! ```

/// One field of a ROS message, in declaration order.
///
/// `offset` is the byte offset of the field within the host Rust struct
/// (typically derived from `core::mem::offset_of!`). Backends that build
/// runtime descriptors use it to compute serializer per-field strides;
/// pure schema consumers (e.g. type-name renderers) may ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Field name as written in the `.msg` IDL.
    pub name: &'static str,
    /// CDR / IDL type of the field.
    pub ty: FieldType,
    /// Byte offset of the field within the host Rust struct.
    pub offset: usize,
}

/// CDR / ROS-IDL type of a single field.
///
/// Covers every variant Cyclone DDS' dynamic-type C API needs for
/// constructing a `dds_topic_descriptor_t` at runtime:
///
/// * primitives (bool, [iu]{8,16,32,64}, f{32,64})
/// * strings (unbounded / bounded; narrow / wide)
/// * nested structs (recurse into a child `&'static [Field]`)
/// * fixed-size arrays (`T[N]`)
/// * unbounded sequences (`sequence<T>`)
/// * bounded sequences (`sequence<T, N>`)
///
/// The recursive variants (`Nested`, `Array`, `Sequence`, `BoundedSequence`)
/// take a `&'static` reference so the entire schema graph stays in `.rodata`
/// with no heap touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// IDL `boolean` — 1 byte, no alignment.
    Bool,
    /// IDL `octet` / `uint8` — 1 byte, no alignment.
    Uint8,
    /// IDL `int8` — 1 byte, no alignment.
    Int8,
    /// IDL `uint16` — 2 bytes, 2-byte aligned.
    Uint16,
    /// IDL `int16` — 2 bytes, 2-byte aligned.
    Int16,
    /// IDL `uint32` — 4 bytes, 4-byte aligned.
    Uint32,
    /// IDL `int32` — 4 bytes, 4-byte aligned.
    Int32,
    /// IDL `uint64` — 8 bytes, 8-byte aligned.
    Uint64,
    /// IDL `int64` — 8 bytes, 8-byte aligned.
    Int64,
    /// IDL `float` / `float32` — 4 bytes, 4-byte aligned.
    Float32,
    /// IDL `double` / `float64` — 8 bytes, 8-byte aligned.
    Float64,
    /// Unbounded `string` (UTF-8 narrow).
    String,
    /// Unbounded `wstring` (UTF-16 wide).
    WString,
    /// Bounded `string<N>` (UTF-8 narrow, max `N` bytes excluding null).
    BoundedString(usize),
    /// Bounded `wstring<N>` (UTF-16 wide, max `N` code units).
    BoundedWString(usize),
    /// Nested struct field; the inner slice is the child's schema.
    Nested(&'static NestedType),
    /// Fixed-size array `T[N]`.
    Array(usize, &'static FieldType),
    /// Unbounded `sequence<T>`.
    Sequence(&'static FieldType),
    /// Bounded `sequence<T, N>`.
    BoundedSequence(usize, &'static FieldType),
}

/// Metadata for a nested struct field.
///
/// Carried by [`FieldType::Nested`] so the runtime descriptor builder can
/// recurse into the child with the correct ROS type name (Cyclone DDS uses
/// it to dedupe identical nested types in the registry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NestedType {
    /// Full ROS type name of the nested struct, e.g. `"builtin_interfaces/msg/Time"`.
    pub type_name: &'static str,
    /// Schema of the nested struct's fields.
    pub fields: &'static [Field],
}

/// Trait implemented by every generated ROS message type for runtime
/// introspection.
///
/// Provides the ROS type name plus the static field schema. Implementors
/// also typically implement [`crate::Serialize`] + [`crate::Deserialize`]
/// for the CDR fast path; this trait is the *introspection* surface used
/// by RMW backends that build type descriptors at runtime.
///
/// All items are `&'static`, so the trait is fully usable in `no_std` +
/// alloc-free environments. The blanket bound is just `Sized` — no
/// `Serialize` / `Deserialize` super-bound, so verification-only mirror
/// types (`nros-ghost-types`) and CycloneDDS-only "descriptor probe"
/// types can implement `Message` without dragging the CDR codecs in.
pub trait Message: Sized {
    /// ROS topic-type name in `package/msg/Type` form
    /// (e.g. `"std_msgs/msg/String"`).
    ///
    /// Wire-level DDS encoding (`"std_msgs::msg::dds_::String_"`) is the
    /// concern of the per-RMW topic-name renderer, *not* this trait.
    const TYPE_NAME: &'static str;

    /// Field schema in declaration order.
    const FIELDS: &'static [Field];
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    // ── Fixtures: hand-rolled mirrors of real ROS messages ──────────────
    //
    // These stand in for what the codegen template (in the standalone
    // `nros-cli` repo, K.7.1) will eventually emit for every msg crate.
    // They exist only to exercise the trait surface in isolation; the
    // real generated msg crates pick up the same impls automatically once
    // the codegen template is updated.

    /// Mirrors `std_msgs/msg/Int32` (one primitive field).
    #[repr(C)]
    struct Int32 {
        data: i32,
    }

    impl Message for Int32 {
        const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
        const FIELDS: &'static [Field] = &[Field {
            name: "data",
            ty: FieldType::Int32,
            offset: offset_of!(Int32, data),
        }];
    }

    /// Mirrors `builtin_interfaces/msg/Time` (two primitives).
    #[repr(C)]
    struct Time {
        sec: i32,
        nanosec: u32,
    }

    impl Message for Time {
        const TYPE_NAME: &'static str = "builtin_interfaces/msg/Time";
        const FIELDS: &'static [Field] = &[
            Field {
                name: "sec",
                ty: FieldType::Int32,
                offset: offset_of!(Time, sec),
            },
            Field {
                name: "nanosec",
                ty: FieldType::Uint32,
                offset: offset_of!(Time, nanosec),
            },
        ];
    }

    /// `Time`'s schema, re-exposed as a nested-type descriptor for
    /// recursion testing.
    const TIME_NESTED: NestedType = NestedType {
        type_name: <Time as Message>::TYPE_NAME,
        fields: <Time as Message>::FIELDS,
    };

    /// Mirrors `std_msgs/msg/Header` (nested struct + string + bounded fields).
    #[repr(C)]
    #[allow(dead_code)]
    struct Header {
        stamp: Time,
        frame_id: &'static str, // representative, not real layout
    }

    impl Message for Header {
        const TYPE_NAME: &'static str = "std_msgs/msg/Header";
        const FIELDS: &'static [Field] = &[
            Field {
                name: "stamp",
                ty: FieldType::Nested(&TIME_NESTED),
                offset: offset_of!(Header, stamp),
            },
            Field {
                name: "frame_id",
                ty: FieldType::String,
                offset: offset_of!(Header, frame_id),
            },
        ];
    }

    /// Mirrors a message with every collection-shape variant the runtime
    /// descriptor builder needs to handle.
    #[repr(C)]
    #[allow(dead_code)]
    struct Collections {
        fixed: [i32; 4],
        bytes: &'static [u8],
        bounded_seq: &'static [u8],
        bounded_str: &'static str,
        bounded_wstr: &'static str,
        wide: &'static str,
    }

    const FIXED_I32: FieldType = FieldType::Int32;
    const SEQ_U8: FieldType = FieldType::Uint8;

    impl Message for Collections {
        const TYPE_NAME: &'static str = "test_msgs/msg/Collections";
        const FIELDS: &'static [Field] = &[
            Field {
                name: "fixed",
                ty: FieldType::Array(4, &FIXED_I32),
                offset: offset_of!(Collections, fixed),
            },
            Field {
                name: "bytes",
                ty: FieldType::Sequence(&SEQ_U8),
                offset: offset_of!(Collections, bytes),
            },
            Field {
                name: "bounded_seq",
                ty: FieldType::BoundedSequence(16, &SEQ_U8),
                offset: offset_of!(Collections, bounded_seq),
            },
            Field {
                name: "bounded_str",
                ty: FieldType::BoundedString(32),
                offset: offset_of!(Collections, bounded_str),
            },
            Field {
                name: "bounded_wstr",
                ty: FieldType::BoundedWString(8),
                offset: offset_of!(Collections, bounded_wstr),
            },
            Field {
                name: "wide",
                ty: FieldType::WString,
                offset: offset_of!(Collections, wide),
            },
        ];
    }

    // ── Tests: shape of the public surface ──────────────────────────────

    #[test]
    fn message_consts_visible_in_const_context() {
        // If `TYPE_NAME` / `FIELDS` weren't `const`, this wouldn't compile.
        const NAME: &str = <Int32 as Message>::TYPE_NAME;
        const FIELDS: &[Field] = <Int32 as Message>::FIELDS;
        assert_eq!(NAME, "std_msgs/msg/Int32");
        assert_eq!(FIELDS.len(), 1);
    }

    #[test]
    fn primitive_field_round_trip() {
        let f = Int32::FIELDS[0];
        assert_eq!(f.name, "data");
        assert!(matches!(f.ty, FieldType::Int32));
        assert_eq!(f.offset, 0);
    }

    #[test]
    fn multi_field_offsets_match_struct_layout() {
        let fields = Time::FIELDS;
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "sec");
        assert_eq!(fields[1].name, "nanosec");
        // sec is at offset 0, nanosec immediately after on a #[repr(C)] {i32, u32}.
        assert_eq!(fields[0].offset, 0);
        assert_eq!(fields[1].offset, 4);
    }

    #[test]
    fn nested_field_recurses_into_child_schema() {
        let fields = Header::FIELDS;
        assert_eq!(fields.len(), 2);
        match fields[0].ty {
            FieldType::Nested(nested) => {
                assert_eq!(nested.type_name, "builtin_interfaces/msg/Time");
                assert_eq!(nested.fields.len(), 2);
                assert_eq!(nested.fields[0].name, "sec");
            }
            _ => panic!("expected Nested variant"),
        }
        assert!(matches!(fields[1].ty, FieldType::String));
    }

    #[test]
    fn collection_variants_cover_array_sequence_bounded_string() {
        let fields = Collections::FIELDS;
        assert_eq!(fields.len(), 6);

        assert!(matches!(
            fields[0].ty,
            FieldType::Array(4, inner) if matches!(*inner, FieldType::Int32),
        ));
        assert!(matches!(
            fields[1].ty,
            FieldType::Sequence(inner) if matches!(*inner, FieldType::Uint8),
        ));
        assert!(matches!(
            fields[2].ty,
            FieldType::BoundedSequence(16, inner) if matches!(*inner, FieldType::Uint8),
        ));
        assert!(matches!(fields[3].ty, FieldType::BoundedString(32)));
        assert!(matches!(fields[4].ty, FieldType::BoundedWString(8)));
        assert!(matches!(fields[5].ty, FieldType::WString));
    }

    #[test]
    fn field_and_fieldtype_are_copy_and_eq() {
        // Compile-time check via trait bound: forces Copy + Eq.
        fn assert_copy_eq<T: Copy + Eq>() {}
        assert_copy_eq::<Field>();
        assert_copy_eq::<FieldType>();
        assert_copy_eq::<NestedType>();

        // Spot-check Eq equality.
        let a = Field {
            name: "x",
            ty: FieldType::Int32,
            offset: 4,
        };
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn all_primitive_variants_constructible() {
        // Smoke: walking a slice of every primitive variant compiles
        // and matches end-to-end. If any variant is removed the match
        // becomes non-exhaustive.
        const PRIMS: &[FieldType] = &[
            FieldType::Bool,
            FieldType::Uint8,
            FieldType::Int8,
            FieldType::Uint16,
            FieldType::Int16,
            FieldType::Uint32,
            FieldType::Int32,
            FieldType::Uint64,
            FieldType::Int64,
            FieldType::Float32,
            FieldType::Float64,
            FieldType::String,
            FieldType::WString,
        ];
        assert_eq!(PRIMS.len(), 13);

        let mut seen = 0u32;
        for ty in PRIMS {
            seen += match ty {
                FieldType::Bool => 1 << 0,
                FieldType::Uint8 => 1 << 1,
                FieldType::Int8 => 1 << 2,
                FieldType::Uint16 => 1 << 3,
                FieldType::Int16 => 1 << 4,
                FieldType::Uint32 => 1 << 5,
                FieldType::Int32 => 1 << 6,
                FieldType::Uint64 => 1 << 7,
                FieldType::Int64 => 1 << 8,
                FieldType::Float32 => 1 << 9,
                FieldType::Float64 => 1 << 10,
                FieldType::String => 1 << 11,
                FieldType::WString => 1 << 12,
                FieldType::BoundedString(_)
                | FieldType::BoundedWString(_)
                | FieldType::Nested(_)
                | FieldType::Array(..)
                | FieldType::Sequence(_)
                | FieldType::BoundedSequence(..) => 0,
            };
        }
        assert_eq!(seen, (1u32 << 13) - 1, "every primitive variant matched");
    }
}
