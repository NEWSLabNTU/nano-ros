//! Runtime descriptor builder.
//!
//! [`DescriptorBuilder::build`] turns a [`nros_serdes::Message`]'s
//! static [`Field`](nros_serdes::Field) schema into a Cyclone DDS
//! `dds_topic_descriptor_t *` by:
//!
//! 1. Walking the schema recursively, flattening every
//!    `&'static FieldType` reference into a stable index in the
//!    `kinds[]` table.
//! 2. Building the top-level `fields[]` table.
//! 3. Calling the C++ bridge in
//!    [`crate::bridge`] to construct the Cyclone descriptor.
//! 4. Returning the resulting opaque pointer.
//!
//! The walker is `no_std`, alloc-free: it borrows fixed-capacity
//! stack arrays (`MAX_FIELDS`, `MAX_KINDS`) sized at compile time
//! by `option_env!` knobs.
//!
//! # Bounded sizing
//!
//! * [`MAX_FIELDS`] — max top-level fields per message. Default 64.
//!   Override at compile time via `NROS_CYCLONEDDS_MAX_FIELDS`.
//! * [`MAX_KINDS`] — max flattened kinds (fields + every nested kind
//!   reached via recursion). Default 256. Override via
//!   `NROS_CYCLONEDDS_MAX_KINDS`.
//! * [`MAX_NESTED_DEPTH`] — recursion ceiling on `Nested` walks.
//!   Default 8. Override via `NROS_CYCLONEDDS_MAX_NESTED_DEPTH`.
//!
//! Overflow returns [`BuildError::RegistryFull`] (for the registry's
//! type-table) or [`BuildError::NestedDepthExceeded`] / a flat-table
//! overflow variant — never panics.

use core::ffi::{c_char, c_int, c_void};

use nros_serdes::schema::{Field, FieldType, Message, NestedType};

use crate::bridge::{
    BridgeError, NrosFieldDescriptor, NrosFieldKindDescriptor,
    nros_cyclonedds_build_descriptor_from_schema,
};

/// Maximum number of top-level fields per message. Compile-time
/// knob: `NROS_CYCLONEDDS_MAX_FIELDS=<N>`. Default 64.
pub const MAX_FIELDS: usize = parse_env_usize(option_env!("NROS_CYCLONEDDS_MAX_FIELDS"), 64);

/// Maximum number of flattened `FieldType` entries in `kinds[]`
/// (top-level fields + every nested `FieldType` reached via
/// recursion). Compile-time knob: `NROS_CYCLONEDDS_MAX_KINDS=<N>`.
/// Default 256.
pub const MAX_KINDS: usize = parse_env_usize(option_env!("NROS_CYCLONEDDS_MAX_KINDS"), 256);

/// Recursion ceiling on `FieldType::Nested` walks. Default 8.
/// Compile-time knob: `NROS_CYCLONEDDS_MAX_NESTED_DEPTH=<N>`.
pub const MAX_NESTED_DEPTH: usize =
    parse_env_usize(option_env!("NROS_CYCLONEDDS_MAX_NESTED_DEPTH"), 8);

const _: () = assert!(MAX_FIELDS >= 1, "MAX_FIELDS must be at least 1");
const _: () = assert!(MAX_KINDS >= 1, "MAX_KINDS must be at least 1");
const _: () = assert!(MAX_NESTED_DEPTH >= 1, "MAX_NESTED_DEPTH must be at least 1");

/// Per-name buffer size for the on-stack NUL-terminated copies the
/// builder hands to the C bridge. 64 bytes covers every ROS field
/// name (typically ≤ 32) and every Cyclone-mangled type name
/// (`pkg::msg::dds_::Type_`, ≤ ~60 bytes). Total stack budget:
/// `(MAX_FIELDS + MAX_KINDS) × NAME_SLOT_LEN` — 20 KiB at default
/// (64 + 256 = 320 slots × 64 B). Tune via the MAX_* knobs if your
/// schema's names exceed this.
pub const NAME_SLOT_LEN: usize = 64;

/// Per-type-name buffer size for the top-level message name. Larger
/// than [`NAME_SLOT_LEN`] because the message's full ROS path
/// (`my_long_pkg/msg/SomeNestedThing`) can run to ~80 bytes in the
/// wild; 256 still rounds up generously.
pub const TYPE_NAME_BUF_LEN: usize = 256;

/// Compile-time `usize` parser. Mirrors the existing
/// `option_env!` knob pattern in `nros-platform`. Returns `default`
/// on `None`; panics in `const` on parse failure (caught at compile
/// time, surfaces as a clean diagnostic at the call site).
const fn parse_env_usize(s: Option<&str>, default: usize) -> usize {
    match s {
        None => default,
        Some(s) => {
            let bytes = s.as_bytes();
            if bytes.is_empty() {
                panic!("env knob set but empty");
            }
            let mut acc: usize = 0;
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b < b'0' || b > b'9' {
                    panic!("non-digit in env knob");
                }
                acc = acc * 10 + (b - b'0') as usize;
                i += 1;
            }
            acc
        }
    }
}

/// Failure modes for [`DescriptorBuilder::build`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    /// The C++ bridge does not yet support this `FieldType` variant
    /// (e.g. `WString` on a Cyclone build without wide-string
    /// support). The Rust walker recognises every variant; this
    /// error always surfaces from the bridge.
    UnsupportedFieldType,
    /// `MAX_NESTED_DEPTH` exceeded while recursing through
    /// [`FieldType::Nested`].
    NestedDepthExceeded,
    /// `MAX_FIELDS` exceeded — top-level message has too many fields.
    FieldsOverflow,
    /// `MAX_KINDS` exceeded — flattening the recursive `FieldType`
    /// graph would not fit. Bump `NROS_CYCLONEDDS_MAX_KINDS`.
    KindsOverflow,
    /// `type_name` was empty (Cyclone rejects empty type names).
    EmptyTypeName,
    /// `M::FIELDS` was empty.
    EmptySchema,
    /// `M::TYPE_NAME` contained an embedded NUL — can't pass through
    /// a C string boundary.
    InvalidTypeName,
    /// Cyclone's `dds_return_t` reported a failure code (passed
    /// through from the bridge).
    DdsError(i32),
    /// The global type registry's [`heapless::FnvIndexMap`] cap was
    /// reached. Bump `NROS_CYCLONEDDS_MAX_TYPES` and rebuild. Surfaces
    /// from [`crate::type_registry::TypeRegistry::get_or_build`] —
    /// kept on `BuildError` so the registry can share one error type.
    RegistryFull,
}

impl BuildError {
    fn from_bridge(code: c_int) -> Self {
        match code {
            x if x == BridgeError::NestedDepthExceeded as i32 => Self::NestedDepthExceeded,
            x if x == BridgeError::UnsupportedFieldType as i32 => Self::UnsupportedFieldType,
            x if x == BridgeError::NullPointer as i32 => Self::InvalidTypeName,
            x if x == BridgeError::EmptySchema as i32 => Self::EmptySchema,
            other => Self::DdsError(other),
        }
    }
}

/// Thin handle around a Cyclone DDS topic-descriptor pointer.
///
/// Constructed via [`DescriptorBuilder::build`] / [`Self::build_raw`];
/// stored in [`crate::type_registry::TypeRegistry`] as an opaque
/// `*const c_void`. The underlying Cyclone descriptor is allocated
/// from `ddsrt_malloc` (Phase 177.22's pre-budgeted heap on embedded
/// targets) — the registry owns the lifetime; do not free directly.
pub struct DescriptorBuilder;

/// Opaque pointer to a Cyclone DDS `dds_topic_descriptor_t`. Returned
/// by the bridge; consumed by `dds_create_topic`. Treated as
/// `*const c_void` throughout this crate so the Rust side never needs
/// the (absent) `cyclonedds-sys` bindgen output.
pub type DescriptorPtr = *const c_void;

impl DescriptorBuilder {
    /// Build a descriptor for `M` and return the resulting pointer.
    pub fn build<M: Message>() -> Result<DescriptorPtr, BuildError> {
        Self::build_raw(M::TYPE_NAME, M::FIELDS)
    }

    /// Build a descriptor from a raw `(type_name, fields)` pair. Used
    /// internally by [`Self::build`]; also useful for tests that don't
    /// want to gin up a full `impl Message`.
    pub fn build_raw(
        type_name: &'static str,
        fields: &'static [Field],
    ) -> Result<DescriptorPtr, BuildError> {
        if type_name.is_empty() {
            return Err(BuildError::EmptyTypeName);
        }
        if fields.is_empty() {
            return Err(BuildError::EmptySchema);
        }
        if fields.len() > MAX_FIELDS {
            return Err(BuildError::FieldsOverflow);
        }

        // The C bridge consumes NUL-terminated `*const c_char`s.
        // Rust string literals (and codegen-emitted `&'static str`
        // constants) are NOT guaranteed NUL-terminated, so we
        // normalise into fixed-size on-stack buffers here. Inputs
        // that already happen to be NUL-terminated (one trailing
        // `\0`) are accepted by trimming the terminator before
        // copying — this also matches future codegen that emits
        // `c"…"` literals via `CStr::to_bytes`.
        let mut type_name_buf = [0u8; TYPE_NAME_BUF_LEN];
        copy_to_buf(type_name, &mut type_name_buf)?;

        // Flatten the schema into bounded stack arrays.
        let mut walker = SchemaWalker::new();
        let mut field_descs = [const { NrosFieldDescriptor::zero() }; MAX_FIELDS];
        // Per-field name buffers + per-nested-kind type-name buffers.
        // Both bounded by their respective static caps to keep the
        // stack footprint deterministic. Per-name slot = 64 B (ROS
        // field names ≤ 32 char; mangled Cyclone type names ≤ ~60).
        // Total: MAX_FIELDS*64 + MAX_KINDS*64 = (64 + 256)*64 = 20 KiB
        // worst case at default sizing — large for an embedded stack
        // but bounded; bump NROS_CYCLONEDDS_MAX_{FIELDS,KINDS} only
        // if your schemas exceed the defaults.
        let mut field_names = [const { [0u8; NAME_SLOT_LEN] }; MAX_FIELDS];
        let mut nested_names = [const { [0u8; NAME_SLOT_LEN] }; MAX_KINDS];

        for (i, f) in fields.iter().enumerate() {
            copy_to_buf(f.name, &mut field_names[i])?;
            let kind_idx = walker.push_field_type(&f.ty, &mut nested_names, 0)? as u32;
            field_descs[i] = NrosFieldDescriptor {
                name: field_names[i].as_ptr() as *const c_char,
                offset: f.offset as u32,
                kind: kind_idx,
            };
        }

        let mut err_code: c_int = 0;
        let descriptor = unsafe {
            nros_cyclonedds_build_descriptor_from_schema(
                type_name_buf.as_ptr() as *const c_char,
                field_descs.as_ptr(),
                fields.len() as u32,
                walker.kinds.as_ptr(),
                walker.kind_count as u32,
                &mut err_code,
            )
        };

        if descriptor.is_null() {
            return Err(BuildError::from_bridge(err_code));
        }
        Ok(descriptor)
    }
}

impl NrosFieldDescriptor {
    /// `const`-constructable zero value for fixed-array
    /// initialisation. Sentinel — overwritten by the walker.
    const fn zero() -> Self {
        Self {
            name: core::ptr::null(),
            offset: 0,
            kind: 0,
        }
    }
}

impl NrosFieldKindDescriptor {
    const fn zero() -> Self {
        Self {
            kind: 0,
            _pad: [0; 3],
            bound: 0,
            inner: 0,
            nested_name: core::ptr::null(),
        }
    }
}

/// Internal: flattens `&'static FieldType` recursion into a
/// bounded `kinds[]` table.
struct SchemaWalker {
    kinds: [NrosFieldKindDescriptor; MAX_KINDS],
    kind_count: usize,
}

impl SchemaWalker {
    fn new() -> Self {
        Self {
            kinds: [const { NrosFieldKindDescriptor::zero() }; MAX_KINDS],
            kind_count: 0,
        }
    }

    /// Append a kind entry for `ty`, recursing into nested types.
    /// Returns the index in `kinds[]` of the appended entry.
    fn push_field_type(
        &mut self,
        ty: &FieldType,
        nested_names: &mut [[u8; NAME_SLOT_LEN]; MAX_KINDS],
        depth: usize,
    ) -> Result<usize, BuildError> {
        if depth >= MAX_NESTED_DEPTH {
            return Err(BuildError::NestedDepthExceeded);
        }
        if self.kind_count >= MAX_KINDS {
            return Err(BuildError::KindsOverflow);
        }
        let my_idx = self.kind_count;
        self.kind_count += 1;
        // Pre-fill with the simple primitives; complex variants
        // overwrite the entry below.
        self.kinds[my_idx] = match ty {
            FieldType::Bool => simple(0),
            FieldType::Uint8 => simple(1),
            FieldType::Int8 => simple(2),
            FieldType::Uint16 => simple(3),
            FieldType::Int16 => simple(4),
            FieldType::Uint32 => simple(5),
            FieldType::Int32 => simple(6),
            FieldType::Uint64 => simple(7),
            FieldType::Int64 => simple(8),
            FieldType::Float32 => simple(9),
            FieldType::Float64 => simple(10),
            FieldType::String => simple(11),
            FieldType::WString => simple(12),
            FieldType::BoundedString(n) => with_bound(13, *n as u32),
            FieldType::BoundedWString(n) => with_bound(14, *n as u32),
            FieldType::Nested(_)
            | FieldType::Array(_, _)
            | FieldType::Sequence(_)
            | FieldType::BoundedSequence(_, _) => NrosFieldKindDescriptor::zero(),
        };

        match ty {
            FieldType::Nested(nested) => {
                let NestedType { type_name, fields } = *nested;
                // Pack `type_name` into a stack-owned NUL-terminated
                // buffer slot (one per kind index) so the bridge can
                // read it.
                copy_to_buf(type_name, &mut nested_names[my_idx])?;
                // Recurse: each child field's `FieldType` becomes an
                // entry in `kinds[]`. We record `inner` as the index
                // of the first child; the bridge walks `bound` (=
                // fields.len()) children sequentially.
                let first_child = self.kind_count as u32;
                for child in fields.iter() {
                    self.push_field_type(&child.ty, nested_names, depth + 1)?;
                }
                self.kinds[my_idx] = NrosFieldKindDescriptor {
                    kind: 15,
                    _pad: [0; 3],
                    bound: fields.len() as u32,
                    inner: first_child,
                    nested_name: nested_names[my_idx].as_ptr() as *const c_char,
                };
            }
            FieldType::Array(n, inner) => {
                let inner_idx = self.push_field_type(inner, nested_names, depth + 1)? as u32;
                self.kinds[my_idx] = NrosFieldKindDescriptor {
                    kind: 16,
                    _pad: [0; 3],
                    bound: *n as u32,
                    inner: inner_idx,
                    nested_name: core::ptr::null(),
                };
            }
            FieldType::Sequence(inner) => {
                let inner_idx = self.push_field_type(inner, nested_names, depth + 1)? as u32;
                self.kinds[my_idx] = NrosFieldKindDescriptor {
                    kind: 17,
                    _pad: [0; 3],
                    bound: 0,
                    inner: inner_idx,
                    nested_name: core::ptr::null(),
                };
            }
            FieldType::BoundedSequence(n, inner) => {
                let inner_idx = self.push_field_type(inner, nested_names, depth + 1)? as u32;
                self.kinds[my_idx] = NrosFieldKindDescriptor {
                    kind: 18,
                    _pad: [0; 3],
                    bound: *n as u32,
                    inner: inner_idx,
                    nested_name: core::ptr::null(),
                };
            }
            // Primitives + strings already filled in above.
            _ => {}
        }

        Ok(my_idx)
    }
}

fn simple(tag: u8) -> NrosFieldKindDescriptor {
    NrosFieldKindDescriptor {
        kind: tag,
        _pad: [0; 3],
        bound: 0,
        inner: 0,
        nested_name: core::ptr::null(),
    }
}

fn with_bound(tag: u8, bound: u32) -> NrosFieldKindDescriptor {
    NrosFieldKindDescriptor {
        kind: tag,
        _pad: [0; 3],
        bound,
        inner: 0,
        nested_name: core::ptr::null(),
    }
}

/// Copy `s` into `buf` and NUL-terminate.
///
/// Accepts both plain `&str` (no trailing NUL) and pre-NUL-terminated
/// strings (one trailing `\0`, matching `c"…".to_bytes_with_nul()`).
/// Embedded NULs (other than a single trailing one) are rejected with
/// [`BuildError::InvalidTypeName`].
fn copy_to_buf(s: &str, buf: &mut [u8]) -> Result<(), BuildError> {
    let mut bytes = s.as_bytes();
    if let Some((&last, rest)) = bytes.split_last()
        && last == 0
    {
        // Already NUL-terminated; strip the trailing NUL before the
        // embedded-NUL scan.
        bytes = rest;
    }
    if bytes.len() + 1 > buf.len() {
        return Err(BuildError::InvalidTypeName);
    }
    for (i, b) in bytes.iter().enumerate() {
        if *b == 0 {
            return Err(BuildError::InvalidTypeName);
        }
        buf[i] = *b;
    }
    buf[bytes.len()] = 0;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ensure const knobs compile + have sane defaults.
    #[test]
    fn const_knobs_have_defaults() {
        const { assert!(MAX_FIELDS >= 1) };
        const { assert!(MAX_KINDS >= 1) };
        const { assert!(MAX_NESTED_DEPTH >= 1) };
    }

    // Walker covers every FieldType variant — exercises the schema
    // fixture from nros-serdes's docs.
    struct AllVariants;
    impl Message for AllVariants {
        const TYPE_NAME: &'static str = "test_msgs/msg/AllVariants\0";
        const FIELDS: &'static [Field] = ALL_FIELDS;
    }

    const NESTED: NestedType = NestedType {
        type_name: "test_msgs/msg/Inner",
        fields: &[Field {
            name: "x\0",
            ty: FieldType::Int32,
            offset: 0,
        }],
    };
    const PRIM_I32: FieldType = FieldType::Int32;
    const PRIM_U8: FieldType = FieldType::Uint8;

    const ALL_FIELDS: &[Field] = &[
        Field {
            name: "b\0",
            ty: FieldType::Bool,
            offset: 0,
        },
        Field {
            name: "u8\0",
            ty: FieldType::Uint8,
            offset: 1,
        },
        Field {
            name: "i8\0",
            ty: FieldType::Int8,
            offset: 2,
        },
        Field {
            name: "u16\0",
            ty: FieldType::Uint16,
            offset: 4,
        },
        Field {
            name: "i16\0",
            ty: FieldType::Int16,
            offset: 6,
        },
        Field {
            name: "u32\0",
            ty: FieldType::Uint32,
            offset: 8,
        },
        Field {
            name: "i32\0",
            ty: FieldType::Int32,
            offset: 12,
        },
        Field {
            name: "u64\0",
            ty: FieldType::Uint64,
            offset: 16,
        },
        Field {
            name: "i64\0",
            ty: FieldType::Int64,
            offset: 24,
        },
        Field {
            name: "f32\0",
            ty: FieldType::Float32,
            offset: 32,
        },
        Field {
            name: "f64\0",
            ty: FieldType::Float64,
            offset: 40,
        },
        Field {
            name: "s\0",
            ty: FieldType::String,
            offset: 48,
        },
        Field {
            name: "w\0",
            ty: FieldType::WString,
            offset: 56,
        },
        Field {
            name: "bs\0",
            ty: FieldType::BoundedString(32),
            offset: 64,
        },
        Field {
            name: "bw\0",
            ty: FieldType::BoundedWString(16),
            offset: 72,
        },
        Field {
            name: "n\0",
            ty: FieldType::Nested(&NESTED),
            offset: 80,
        },
        Field {
            name: "arr\0",
            ty: FieldType::Array(4, &PRIM_I32),
            offset: 88,
        },
        Field {
            name: "seq\0",
            ty: FieldType::Sequence(&PRIM_U8),
            offset: 104,
        },
        Field {
            name: "bseq\0",
            ty: FieldType::BoundedSequence(8, &PRIM_U8),
            offset: 112,
        },
    ];

    #[test]
    fn descriptor_builder_accepts_every_field_variant() {
        let ptr = DescriptorBuilder::build::<AllVariants>().expect("build descriptor");
        assert!(!ptr.is_null());
    }

    #[test]
    fn empty_schema_rejected() {
        struct Empty;
        impl Message for Empty {
            const TYPE_NAME: &'static str = "test_msgs/msg/Empty\0";
            const FIELDS: &'static [Field] = &[];
        }
        assert_eq!(
            DescriptorBuilder::build::<Empty>().unwrap_err(),
            BuildError::EmptySchema
        );
    }

    #[test]
    fn empty_type_name_rejected() {
        assert_eq!(
            DescriptorBuilder::build_raw("", ALL_FIELDS).unwrap_err(),
            BuildError::EmptyTypeName
        );
    }

    #[test]
    fn bridge_error_codes_round_trip_through_build_error() {
        use crate::bridge::{BridgeError, test_stub::FORCED_ERROR};
        use core::sync::atomic::Ordering;

        let cases = [
            (
                BridgeError::NestedDepthExceeded as i32,
                BuildError::NestedDepthExceeded,
            ),
            (
                BridgeError::UnsupportedFieldType as i32,
                BuildError::UnsupportedFieldType,
            ),
            (BridgeError::NullPointer as i32, BuildError::InvalidTypeName),
            (BridgeError::EmptySchema as i32, BuildError::EmptySchema),
            (-77, BuildError::DdsError(-77)),
        ];
        for (code, expected) in cases {
            FORCED_ERROR.store(code, Ordering::SeqCst);
            let err = DescriptorBuilder::build::<AllVariants>().unwrap_err();
            assert_eq!(err, expected, "code {code}");
        }
    }
}
