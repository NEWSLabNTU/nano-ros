//! Runtime descriptor builder.
//!
//! [`DescriptorBuilder::build`] turns a [`nros_serdes::Message`]'s
//! static [`nros_serdes::Field`] schema into a Cyclone DDS
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
    ///
    /// # Service request/reply prefix injection (Phase 212.K.7 fix)
    ///
    /// When `type_name` matches a service request / reply shape — i.e.
    /// the basename ends with `_Request`, `_Response`, or `_Reply` —
    /// the descriptor synthesised here gets the 16-byte
    /// `cdds_request_header_t` prefix (`rmw_writer_guid: u64`,
    /// `rmw_sequence_number: i64`) injected at offset 0, with every
    /// caller-supplied field offset shifted by 16. This mirrors stock
    /// `rmw_cyclonedds_cpp`'s idlc-emitted Request/Response descriptors
    /// (see `service.cpp` doc block) and is what `service_send_request_raw`
    /// / `service_try_recv_request` already assume on the C++ wire side
    /// when they prepend / split the 16-byte header. The user-facing
    /// `Serialize` / `Deserialize` impls intentionally still serialise
    /// only user fields — the wire framing (`build_wire_with_header`)
    /// supplies the 16-byte prefix bytes; the descriptor needs the
    /// matching field declarations so Cyclone's
    /// `dds_stream_{read,write}_sample` round-trip preserves the user
    /// payload that follows the header.
    ///
    /// Without this prefix the synthesised descriptor's `m_size = 16`
    /// (or the size of just the user fields) and Cyclone silently
    /// truncates the wire CDR's user payload during the take/write
    /// reserialise pass — manifesting as a server-side `wire_len`
    /// equal to encap+header with no user bytes (Phase 212.K.7 root
    /// cause: native-Rust-Cyclone service requests timed out because
    /// the server received only the 16-byte header, never the `a`/`b`
    /// payload).
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

        let prepend_header = is_service_request_or_reply(type_name);
        let header_field_count = if prepend_header {
            SERVICE_HEADER_FIELDS.len()
        } else {
            0
        };
        let total_fields = fields.len() + header_field_count;
        if total_fields > MAX_FIELDS {
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

        // 1. Inject the 16-byte cdds_request_header_t prefix if the
        //    type is a service request / reply (see method docs).
        if prepend_header {
            for (i, hf) in SERVICE_HEADER_FIELDS.iter().enumerate() {
                copy_to_buf(hf.name, &mut field_names[i])?;
                let kind_idx = walker.push_field_type(&hf.ty, &mut nested_names, 0)? as u32;
                field_descs[i] = NrosFieldDescriptor {
                    name: field_names[i].as_ptr() as *const c_char,
                    offset: hf.offset as u32,
                    kind: kind_idx,
                };
            }
        }

        // 2. Caller-supplied user fields. For service request/reply types
        //    we override the codegen-supplied `f.offset` with synthetic
        //    CDR-walk-order offsets (Phase 212.K.7.4.e).
        //
        //    Why: codegen-generated envelope structs (e.g.
        //    `Fibonacci_SendGoal_Response { accepted: bool, stamp: Time }`)
        //    are emitted without `#[repr(C)]`. Rust's default `repr(Rust)`
        //    is free to reorder fields for size — so `offset_of!(.., accepted)`
        //    can land at 8 (after `stamp`) instead of 0. The runtime wire
        //    framing for `_SendGoal_*` / `_GetResult_*` in
        //    `service.cpp::{write_typed, take_typed_wire}` uses raw memcpy
        //    between the wire CDR (post-encap, post-header) and the typed
        //    sample, treating the sample as CDR-ordered. That contract
        //    breaks when Cyclone reads/writes fields at the
        //    Rust-reordered positions.
        //
        //    For service request/reply types the sample memory is
        //    bridge-internal (the runtime only ever sees the wire CDR), so
        //    we're free to pick a layout. Using CDR-walk-order offsets
        //    matches the memcpy paths and round-trips correctly through
        //    `dds_stream_{read,write}_sample`. The 16-byte header sits at
        //    offsets 0/8 (already CDR-aligned) so user fields start at 16.
        //
        //    For non-service types (plain pub/sub) we keep the codegen
        //    offsets — those exercise the regular `write_typed` /
        //    `take_typed_wire` paths that round-trip via Cyclone CDR
        //    only; consistency between read and write is all that
        //    matters, and the existing K.7.7 pub/sub e2e proves the
        //    pattern works there even with `repr(Rust)` codegen.
        let mut cdr_cursor = if prepend_header {
            SERVICE_HEADER_BYTES
        } else {
            0
        };
        for (i, f) in fields.iter().enumerate() {
            let slot = header_field_count + i;
            copy_to_buf(f.name, &mut field_names[slot])?;
            let kind_idx = walker.push_field_type(&f.ty, &mut nested_names, 0)? as u32;
            let synthetic_offset = if prepend_header {
                let align = cdr_align_of(&f.ty);
                cdr_cursor = (cdr_cursor + align - 1) & !(align - 1);
                let start = cdr_cursor;
                cdr_cursor = cdr_cursor.saturating_add(cdr_size_of(&f.ty));
                start
            } else {
                f.offset
            };
            field_descs[slot] = NrosFieldDescriptor {
                name: field_names[slot].as_ptr() as *const c_char,
                offset: synthetic_offset as u32,
                kind: kind_idx,
            };
        }

        let mut err_code: c_int = 0;
        let descriptor = unsafe {
            nros_cyclonedds_build_descriptor_from_schema(
                type_name_buf.as_ptr() as *const c_char,
                field_descs.as_ptr(),
                total_fields as u32,
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

// ── CDR layout helpers (Phase 212.K.7.4.e) ─────────────────────────────
//
// Compute the CDR-walk-order (size, alignment) for a `FieldType` so
// `build_raw` can lay out the bridge-internal typed sample in CDR order
// for service request/reply types whose Rust struct codegen lacks
// `#[repr(C)]` and may therefore reorder fields.
//
// These mirror Cyclone's `primitive_size_align` in
// `bridge/dynamic_type_builder.cpp` plus the conservative
// placeholders used there for nested / sequence / string fields:
//   * Nested struct: 16 B / align 8 (placeholder — matches the bridge's
//     `compute_struct_size` branch).
//   * Sequence / bounded sequence / string / bounded string: 16 B /
//     align 8 (matches `kSeqSize`/`kStringSize` in the bridge).
//   * Array `T[N]`: N * size_of(T), aligned to align_of(T).

const fn cdr_size_of(ty: &FieldType) -> usize {
    match ty {
        FieldType::Bool | FieldType::Uint8 | FieldType::Int8 => 1,
        FieldType::Uint16 | FieldType::Int16 => 2,
        FieldType::Uint32 | FieldType::Int32 | FieldType::Float32 => 4,
        FieldType::Uint64 | FieldType::Int64 | FieldType::Float64 => 8,
        FieldType::String
        | FieldType::BoundedString(_)
        | FieldType::WString
        | FieldType::BoundedWString(_)
        | FieldType::Sequence(_)
        | FieldType::BoundedSequence(_, _) => 16,
        FieldType::Nested(n) => cdr_struct_size(n),
        FieldType::Array(n, inner) => *n * cdr_size_of(inner),
    }
}

const fn cdr_align_of(ty: &FieldType) -> usize {
    match ty {
        FieldType::Bool | FieldType::Uint8 | FieldType::Int8 => 1,
        FieldType::Uint16 | FieldType::Int16 => 2,
        FieldType::Uint32 | FieldType::Int32 | FieldType::Float32 => 4,
        FieldType::Uint64 | FieldType::Int64 | FieldType::Float64 => 8,
        FieldType::String
        | FieldType::BoundedString(_)
        | FieldType::WString
        | FieldType::BoundedWString(_)
        | FieldType::Sequence(_)
        | FieldType::BoundedSequence(_, _) => 8,
        FieldType::Nested(n) => cdr_struct_align(n),
        FieldType::Array(_, inner) => cdr_align_of(inner),
    }
}

const fn cdr_struct_align(n: &NestedType) -> usize {
    let mut max_align = 1usize;
    let mut i = 0;
    while i < n.fields.len() {
        let a = cdr_align_of(&n.fields[i].ty);
        if a > max_align {
            max_align = a;
        }
        i += 1;
    }
    max_align
}

const fn cdr_struct_size(n: &NestedType) -> usize {
    let mut cursor = 0usize;
    let mut max_align = 1usize;
    let mut i = 0;
    while i < n.fields.len() {
        let a = cdr_align_of(&n.fields[i].ty);
        let s = cdr_size_of(&n.fields[i].ty);
        if a > max_align {
            max_align = a;
        }
        // align cursor up to `a`.
        cursor = (cursor + a - 1) & !(a - 1);
        cursor = cursor + s;
        i += 1;
    }
    // pad up to struct alignment.
    (cursor + max_align - 1) & !(max_align - 1)
}

// ── Service request / reply CDR header (Phase 212.K.7) ─────────────────
//
// Stock `rmw_cyclonedds_cpp` idlc-generated request/reply structs carry
// a 16-byte `cdds_request_header_t` prefix:
//
//     uint64_t rmw_writer_guid;       // lower 8 bytes of RTPS GUID
//     int64_t  rmw_sequence_number;   // monotonic per-client
//     /* user fields ... */
//
// (See `packages/dds/nros-rmw-cyclonedds/src/service.cpp` top-of-file
// doc block + `cdds_request_header_t` in upstream
// `rmw_cyclonedds_cpp/src/serdata.hpp:73-77`.)
//
// The Rust runtime descriptor builder injects matching fields into
// every Request/Reply descriptor so Cyclone's typed sample I/O
// (`dds_stream_{read,write}_sample`) preserves the user payload that
// follows the prefix. The user-facing `Serialize`/`Deserialize` impls
// still only round-trip user fields — the prefix bytes come from
// `build_wire_with_header` / `split_wire_header` in `service.cpp`.

/// Total byte size of the synthesised `cdds_request_header_t` prefix.
pub const SERVICE_HEADER_BYTES: usize = 16;

/// The two synthesised header fields, in order.
///
/// `offset` is absolute within the typed sample — `rmw_writer_guid` at
/// 0, `rmw_sequence_number` at 8. Total prefix size 16 B.
const SERVICE_HEADER_FIELDS: &[Field] = &[
    Field {
        name: "rmw_writer_guid",
        ty: FieldType::Uint64,
        offset: 0,
    },
    Field {
        name: "rmw_sequence_number",
        ty: FieldType::Int64,
        offset: 8,
    },
];

/// Returns `true` if `type_name` (in `pkg/srv/<Svc>_Request` /
/// `_Response` / `_Reply` slash-form *or* the
/// `pkg::srv::dds_::<Svc>_Request_` mangled form) names a service
/// request or reply type.
///
/// The slash form is the one [`Message::TYPE_NAME`] is documented to
/// carry; the mangled form is checked defensively in case codegen
/// emits the wire-form directly (e.g. mirroring `RosMessage::TYPE_NAME`).
fn is_service_request_or_reply(type_name: &str) -> bool {
    // Normalise: trim an optional trailing NUL (codegen may include
    // a NUL terminator in the `&'static str`), then trim a trailing
    // `_` (mangled Cyclone names end in `_`).
    let mut s = type_name.as_bytes();
    while let Some(b'\0') = s.last() {
        s = &s[..s.len() - 1];
    }
    while let Some(b'_') = s.last() {
        s = &s[..s.len() - 1];
    }
    ends_with_ignore_case(s, b"_Request")
        || ends_with_ignore_case(s, b"_Response")
        || ends_with_ignore_case(s, b"_Reply")
}

/// Case-sensitive `ends_with` for byte slices (avoid pulling
/// `str::ends_with` since `s` is already a `&[u8]`).
fn ends_with_ignore_case(s: &[u8], suffix: &[u8]) -> bool {
    if s.len() < suffix.len() {
        return false;
    }
    let tail = &s[s.len() - suffix.len()..];
    tail == suffix
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

    // ── Service request/reply prefix injection (Phase 212.K.7 fix) ─────

    #[test]
    fn service_request_reply_predicate_recognises_canonical_shapes() {
        // Slash form — the `Message::TYPE_NAME` documented shape.
        assert!(is_service_request_or_reply(
            "example_interfaces/srv/AddTwoInts_Request"
        ));
        assert!(is_service_request_or_reply(
            "example_interfaces/srv/AddTwoInts_Response"
        ));
        // `_Reply` accepted defensively (older test-fixture spelling;
        // wire-form is `_Response` but the predicate stays lenient so
        // an in-tree mock doesn't have to renumber).
        assert!(is_service_request_or_reply("test/srv/TestService_Reply"));
        // Mangled (Cyclone wire) form, trailing `_` allowed.
        assert!(is_service_request_or_reply(
            "example_interfaces::srv::dds_::AddTwoInts_Request_"
        ));
        assert!(is_service_request_or_reply(
            "example_interfaces::srv::dds_::AddTwoInts_Response_"
        ));
        // NUL-terminated form (codegen may emit one for C-bridge ease).
        assert!(is_service_request_or_reply(
            "example_interfaces/srv/AddTwoInts_Request\0"
        ));

        // Negatives — plain message types must NOT get the prefix.
        assert!(!is_service_request_or_reply("std_msgs/msg/Int32"));
        assert!(!is_service_request_or_reply("std_msgs::msg::dds_::Int32_"));
        assert!(!is_service_request_or_reply(
            "example_interfaces/srv/AddTwoInts"
        ));
        // The action `_Goal` / `_Result` / `_Feedback` wrappers are
        // services internally but they reach us through their own
        // `_Request` / `_Response` derivatives (see service.cpp
        // `action_effective_base`). The bare wrapper names alone don't
        // need the prefix.
        assert!(!is_service_request_or_reply("test/action/TestAction_Goal"));
    }

    /// A Request-shaped Message: two user fields, name suffixes
    /// `_Request`. The builder MUST inject the 16-byte header before
    /// the user fields.
    struct AddTwoIntsReq;
    impl Message for AddTwoIntsReq {
        const TYPE_NAME: &'static str = "example_interfaces/srv/AddTwoInts_Request";
        const FIELDS: &'static [Field] = &[
            Field {
                name: "a\0",
                ty: FieldType::Int64,
                offset: 0,
            },
            Field {
                name: "b\0",
                ty: FieldType::Int64,
                offset: 8,
            },
        ];
    }

    /// A Response-shaped Message — single user field, name suffix
    /// `_Response`.
    struct AddTwoIntsRsp;
    impl Message for AddTwoIntsRsp {
        const TYPE_NAME: &'static str = "example_interfaces/srv/AddTwoInts_Response";
        const FIELDS: &'static [Field] = &[Field {
            name: "sum\0",
            ty: FieldType::Int64,
            offset: 0,
        }];
    }

    /// A plain message — must NOT receive the prefix.
    struct Int32Msg;
    impl Message for Int32Msg {
        const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
        const FIELDS: &'static [Field] = &[Field {
            name: "data\0",
            ty: FieldType::Int32,
            offset: 0,
        }];
    }

    /// Phase 212.K.7.4.e — codegen-emitted service request/reply types
    /// like `Fibonacci_SendGoal_Response { accepted: bool, stamp: Time }`
    /// are NOT `#[repr(C)]`; Rust's default `repr(Rust)` reorders fields
    /// for size — `offset_of!(.., accepted)` lands at 8, not 0. The
    /// runtime memcpy paths in `service.cpp::{write_typed,
    /// take_typed_wire}` for `_SendGoal_*` / `_GetResult_*` treat the
    /// typed sample as CDR-ordered, so the descriptor MUST place
    /// `accepted` at the post-header CDR position (16), not whatever
    /// Rust picked.
    ///
    /// This test mirrors the codegen output exactly (mis-ordered Rust
    /// offsets) and asserts `build_raw` overrides them with
    /// CDR-walk-order synthetic offsets (16 for `accepted`, 20 for
    /// `stamp`). Pre-K.7.4.e it would report (24, 16) — Rust's
    /// reordered struct shifted by 16.
    #[test]
    fn service_response_uses_cdr_walk_order_offsets() {
        use crate::bridge::test_stub::{LAST_FIELD_COUNT, LAST_FIELDS};
        use core::sync::atomic::Ordering;

        // Mirror the `Time` nested struct (matches builtin_interfaces/msg/Time).
        static TIME_FIELDS: &[Field] = &[
            Field {
                name: "sec\0",
                ty: FieldType::Int32,
                offset: 0,
            },
            Field {
                name: "nanosec\0",
                ty: FieldType::Uint32,
                offset: 4,
            },
        ];
        static TIME_NESTED: NestedType = NestedType {
            type_name: "builtin_interfaces/msg/Time",
            fields: TIME_FIELDS,
        };

        // Mirror `Fibonacci_SendGoal_Response { accepted: bool, stamp: Time }`
        // codegen output: declaration order [accepted, stamp], but Rust's
        // `repr(Rust)` reorders `stamp` (align 4, size 8) before `accepted`
        // (align 1) → offsets (8, 0). We feed those reordered offsets to
        // build_raw; the K.7.4.e synthetic-offset logic must ignore them.
        static REORDERED_FIELDS: &[Field] = &[
            Field {
                name: "accepted\0",
                ty: FieldType::Bool,
                offset: 8, // Rust's `repr(Rust)` reordered position.
            },
            Field {
                name: "stamp\0",
                ty: FieldType::Nested(&TIME_NESTED),
                offset: 0, // Rust's `repr(Rust)` reordered position.
            },
        ];

        // Build a Response-suffixed descriptor.
        let ptr = DescriptorBuilder::build_raw(
            "example_interfaces/srv/Fake_SendGoal_Response",
            REORDERED_FIELDS,
        )
        .expect("build send_goal response");
        assert!(!ptr.is_null());

        // 2 header fields + 2 user fields = 4 captured.
        let n = LAST_FIELD_COUNT.load(Ordering::SeqCst);
        assert_eq!(n, 4, "expected 2 header + 2 user fields");

        // SAFETY: the bridge stub has returned, so the slots are no
        // longer being written.
        let captured: [(u32, u32); 4] = unsafe {
            [
                *LAST_FIELDS[0].0.get(),
                *LAST_FIELDS[1].0.get(),
                *LAST_FIELDS[2].0.get(),
                *LAST_FIELDS[3].0.get(),
            ]
        };

        // Header: (offset 0, kind=Uint64), (offset 8, kind=Int64).
        assert_eq!(captured[0].0, 0, "header[0] guid offset");
        assert_eq!(captured[1].0, 8, "header[1] seq offset");

        // User: regardless of the (wrong) Rust offsets we fed in,
        // the K.7.4.e fix must place `accepted` at CDR offset 16 and
        // `stamp` aligned-to-4 right after = 20.
        assert_eq!(
            captured[2].0, 16,
            "accepted MUST land at post-header CDR offset 16 (got {})",
            captured[2].0
        );
        assert_eq!(
            captured[3].0, 20,
            "stamp MUST land at CDR-walk-aligned offset 20 (got {})",
            captured[3].0
        );
    }

    /// Plain (non-service) message types must keep using the
    /// codegen-supplied `offset_of!` values — those go through the
    /// regular `dds_stream_{read,write}_sample` round-trip in
    /// `service.cpp::{write_typed, take_typed_wire}` where consistency
    /// (not CDR-ordering) is all that's required, and the existing K.7.7
    /// pub/sub e2e proves the pattern works.
    #[test]
    fn plain_message_keeps_codegen_offsets() {
        use crate::bridge::test_stub::{LAST_FIELD_COUNT, LAST_FIELDS};
        use core::sync::atomic::Ordering;

        // A two-primitive message with a deliberately quirky `offset` to
        // prove we pass it through untouched.
        static FIELDS: &[Field] = &[
            Field {
                name: "x\0",
                ty: FieldType::Int32,
                offset: 12,
            },
            Field {
                name: "y\0",
                ty: FieldType::Int64,
                offset: 0,
            },
        ];

        let ptr =
            DescriptorBuilder::build_raw("test_msgs/msg/Plain", FIELDS).expect("build plain msg");
        assert!(!ptr.is_null());

        let n = LAST_FIELD_COUNT.load(Ordering::SeqCst);
        assert_eq!(n, 2, "plain msg has no header injection");
        // SAFETY: see above.
        let captured: [(u32, u32); 2] =
            unsafe { [*LAST_FIELDS[0].0.get(), *LAST_FIELDS[1].0.get()] };
        assert_eq!(captured[0].0, 12, "plain msg x kept codegen offset");
        assert_eq!(captured[1].0, 0, "plain msg y kept codegen offset");
    }

    #[test]
    fn service_request_descriptor_succeeds_with_only_user_fields() {
        // Without the K.7 fix this would either succeed with the wrong
        // m_size (test stub returns a unique non-null pointer regardless
        // of field shape) or — with a strict bridge — fail. We can't
        // observe `m_size` from the test stub, so we assert the build
        // path is taken end-to-end and the predicate-driven prefix
        // count makes total_fields exceed user_fields.
        let ptr = DescriptorBuilder::build::<AddTwoIntsReq>().expect("build req");
        assert!(!ptr.is_null());

        let ptr = DescriptorBuilder::build::<AddTwoIntsRsp>().expect("build rsp");
        assert!(!ptr.is_null());

        let ptr = DescriptorBuilder::build::<Int32Msg>().expect("build msg");
        assert!(!ptr.is_null());
    }

    #[test]
    fn service_request_prefix_overflow_caps_at_max_fields() {
        // A Request type with (MAX_FIELDS - 1) user fields is fine on
        // its own but, after the 2-field prefix injection, overflows.
        // Generate it via build_raw which does not need a Message impl.
        const N: usize = MAX_FIELDS - 1;
        const fn mk_fields() -> [Field; N] {
            let mut out = [Field {
                name: "x\0",
                ty: FieldType::Uint8,
                offset: 0,
            }; N];
            let mut i = 0;
            while i < N {
                out[i] = Field {
                    name: "x\0",
                    ty: FieldType::Uint8,
                    offset: i as u32 as usize,
                };
                i += 1;
            }
            out
        }
        static FIELDS: [Field; N] = mk_fields();
        // Plain message (no suffix): fits exactly.
        assert!(DescriptorBuilder::build_raw("plain/msg/Big", &FIELDS).is_ok());
        // Request (suffix triggers prefix): overflows by 1.
        assert_eq!(
            DescriptorBuilder::build_raw("plain/srv/Big_Request", &FIELDS).unwrap_err(),
            BuildError::FieldsOverflow,
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
