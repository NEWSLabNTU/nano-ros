//! C++ bridge declarations.
//!
//! The Cyclone DDS C API (and the C++ headers `dds/ddsi/ddsi_dynamic_type*`)
//! are not exposed through any in-tree Rust `*-sys` crate — `cyclonedds-sys`
//! only ships the build-time link recipe, no bindgen-generated types. To
//! keep the K.7.4 descriptor builder host-language-honest, the actual walk
//! lives on the C++ side in `bridge/dynamic_type_builder.cpp` and exposes
//! a single `extern "C"` entry that the Rust shim calls with the schema
//! laid out for ABI stability.
//!
//! The ABI mirrors [`nros_serdes::schema`]: each `FieldType` variant
//! maps to a tag from [`FieldKind`] plus an `inner` index (for the
//! recursive variants — `Array` / `Sequence` / `BoundedSequence` /
//! `Nested`) into a shared `kinds[]` table that flattens the
//! `&'static FieldType` references the schema uses.
//!
//! The C++ TU is responsible for:
//!
//! 1. Walking the flattened `FieldDescriptor[]` + `FieldKindDescriptor[]`
//!    arrays.
//! 2. Calling Cyclone's `ddsi_dynamic_type_*` constructors to build
//!    nested types bottom-up.
//! 3. Finalising into a `dds_topic_descriptor_t` allocated from
//!    `ddsrt_malloc`.
//! 4. Returning the pointer (or NULL on error, with `*out_err` set to
//!    a [`BridgeError`] code).
//!
//! See `bridge/dynamic_type_builder.cpp` for the implementation.

use core::ffi::{c_char, c_int};

/// Tags every recognised [`nros_serdes::schema::FieldType`] variant in
/// a stable C ABI representation. Mirrors `enum NrosFieldKind` in
/// `bridge/dynamic_type_builder.cpp`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Bool = 0,
    Uint8 = 1,
    Int8 = 2,
    Uint16 = 3,
    Int16 = 4,
    Uint32 = 5,
    Int32 = 6,
    Uint64 = 7,
    Int64 = 8,
    Float32 = 9,
    Float64 = 10,
    String = 11,
    WString = 12,
    BoundedString = 13,
    BoundedWString = 14,
    /// `Nested(NestedType { type_name, fields })`. The `bound` slot
    /// carries `fields.len()`; `inner` indexes the first child kind in
    /// the flattened `kinds[]` table.
    Nested = 15,
    /// `Array(N, inner)` — `bound = N`, `inner` indexes `kinds[]`.
    Array = 16,
    /// `Sequence(inner)` — `inner` indexes `kinds[]`, `bound = 0`.
    Sequence = 17,
    /// `BoundedSequence(N, inner)` — `bound = N`.
    BoundedSequence = 18,
}

/// Mirror of `struct NrosFieldKindDescriptor` in
/// `bridge/dynamic_type_builder.cpp`.
///
/// Together the `kind` + `bound` + `inner` triple captures every
/// shape of [`nros_serdes::schema::FieldType`]:
///
/// | FieldType | kind | bound | inner | nested_name |
/// |---|---|---|---|---|
/// | `Bool` … `Float64` | matching tag | 0 | 0 | NULL |
/// | `String` / `WString` | `String` / `WString` | 0 | 0 | NULL |
/// | `BoundedString(N)` / `BoundedWString(N)` | matching tag | N | 0 | NULL |
/// | `Nested(NestedType)` | `Nested` | child fields[].len() | first child idx in kinds[] | type_name |
/// | `Array(N, inner)` | `Array` | N | inner kind idx | NULL |
/// | `Sequence(inner)` | `Sequence` | 0 | inner kind idx | NULL |
/// | `BoundedSequence(N, inner)` | `BoundedSequence` | N | inner kind idx | NULL |
///
/// The flattened layout sidesteps Rust's lifetime tracking for the
/// `&'static FieldType` recursion the schema uses internally.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NrosFieldKindDescriptor {
    pub kind: u8,
    pub _pad: [u8; 3],
    pub bound: u32,
    pub inner: u32,
    /// NUL-terminated nested type-name pointer (only valid for
    /// `kind == Nested`; otherwise NULL).
    pub nested_name: *const c_char,
}

// SAFETY: only contains POD + a `*const c_char` pointer that lives in
// `.rodata` (every name in the schema is a `&'static str`). The C++
// bridge reads it from the calling thread; the registry mutex
// serialises Rust-side access. Cross-thread send/sync is therefore
// safe for the descriptor table itself.
unsafe impl Send for NrosFieldKindDescriptor {}
unsafe impl Sync for NrosFieldKindDescriptor {}

/// Mirror of `struct NrosFieldDescriptor` — one entry per field of
/// the top-level message.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NrosFieldDescriptor {
    /// NUL-terminated field name (points into `.rodata`).
    pub name: *const c_char,
    /// Byte offset of the field within the host struct.
    pub offset: u32,
    /// Index into the flattened `kinds[]` table.
    pub kind: u32,
}

unsafe impl Send for NrosFieldDescriptor {}
unsafe impl Sync for NrosFieldDescriptor {}

/// Bridge return codes. Mirrors `enum NrosBridgeError` in
/// `bridge/dynamic_type_builder.cpp`.
///
/// Negative codes from the underlying `dds_return_t` API pass through
/// unchanged (Cyclone uses `< 0` for failure).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    /// Recursion limit exceeded while walking [`FieldKind::Nested`].
    NestedDepthExceeded = -1001,
    /// One of the schema's `FieldType` variants is recognised but
    /// not yet wired through to Cyclone's dynamic-type API (e.g.
    /// `WString` on a build where Cyclone's wide-string variant is
    /// disabled, or `BoundedSequence` on a Cyclone older than the
    /// dynamic-bounded-sequence API).
    UnsupportedFieldType = -1002,
    /// A `*const c_char` was unexpectedly NULL.
    NullPointer = -1003,
    /// `count == 0` — empty schema rejected.
    EmptySchema = -1004,
}

unsafe extern "C" {
    /// C++ bridge entry — see `bridge/dynamic_type_builder.cpp`.
    ///
    /// # Arguments
    ///
    /// * `type_name` — NUL-terminated ROS type name in
    ///   `package/msg/Type` form (the bridge mangles it to the
    ///   `package::msg::dds_::Type_` Cyclone shape internally).
    /// * `fields` / `field_count` — top-level field descriptors.
    /// * `kinds` / `kind_count` — flattened recursive-kind table.
    /// * `out_err` — set to a [`BridgeError`] (or a Cyclone
    ///   `dds_return_t`) on NULL return; left untouched on success.
    ///
    /// # Returns
    ///
    /// Non-NULL `*const dds_topic_descriptor_t` (opaque to Rust) on
    /// success. NULL on failure — read `*out_err` for the reason.
    ///
    /// # Safety
    ///
    /// All pointers must be valid for the duration of the call. The
    /// returned pointer is allocated by Cyclone (`ddsrt_malloc`) and
    /// is owned by the global registry once stored — callers must not
    /// free it.
    pub fn nros_cyclonedds_build_descriptor_from_schema(
        type_name: *const c_char,
        fields: *const NrosFieldDescriptor,
        field_count: u32,
        kinds: *const NrosFieldKindDescriptor,
        kind_count: u32,
        out_err: *mut c_int,
    ) -> *const core::ffi::c_void;

    /// Register a built descriptor with the existing C++
    /// `descriptors.cpp` registry so that the C++ pub/sub paths can
    /// find it via `find_descriptor(type_name)`.
    ///
    /// Mirrors the C entry already used by the static-init `_register.c`
    /// codegen TUs.
    pub fn nros_rmw_cyclonedds_register_descriptor(
        type_name: *const c_char,
        descriptor: *const core::ffi::c_void,
    );
}

// ── Test-only stub ──────────────────────────────────────────────────
//
// `cargo test -p nros-rmw-cyclonedds` runs on a hosted toolchain that
// has no Cyclone DDS C library available. Stub both bridge entries
// out under `#[cfg(test)]` so the registry + builder logic compiles
// and exercises end-to-end without dragging in `libddsc`.

#[cfg(any(test, feature = "bridge-stub"))]
pub mod test_stub {
    use core::{
        cell::UnsafeCell,
        ffi::{c_char, c_int, c_void},
        sync::atomic::{AtomicI32, AtomicUsize, Ordering},
    };

    use super::{NrosFieldDescriptor, NrosFieldKindDescriptor};

    /// Set to a non-zero `BridgeError` code to force the stub to
    /// fail the next call (used by `BuildError` mapping tests).
    pub static FORCED_ERROR: AtomicI32 = AtomicI32::new(0);

    /// Increments each time the stub fakes a successful build. Let
    /// tests assert the registry's cache-hit behaviour.
    pub static BUILD_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Captures the last `(offset, kind)` pair per top-level field passed
    /// to the bridge builder (Phase 212.K.7.4.e — lets unit tests assert
    /// that service request/reply types get the CDR-walk-order synthetic
    /// offsets that match the runtime memcpy paths in
    /// `service.cpp::{write_typed, take_typed_wire}`).
    pub const LAST_FIELDS_CAP: usize = 16;
    pub static LAST_FIELD_COUNT: AtomicUsize = AtomicUsize::new(0);

    /// Sync newtype around `UnsafeCell<(u32, u32)>`. Captures per-field
    /// `(offset, kind)` from the bridge stub. Single-threaded tests only;
    /// writers are serialised by `BUILD_COUNTER` so readers (in the test
    /// body after `build_raw` returns) observe stable values.
    #[repr(transparent)]
    pub struct FieldCapture(pub UnsafeCell<(u32, u32)>);

    // SAFETY: see `FieldCapture` doc comment — single-threaded test use only.
    unsafe impl Sync for FieldCapture {}

    /// `(offset, kind)` per captured field. Tests inspect after the
    /// corresponding `build_raw` call returns.
    pub static LAST_FIELDS: [FieldCapture; LAST_FIELDS_CAP] =
        [const { FieldCapture(UnsafeCell::new((0u32, 0u32))) }; LAST_FIELDS_CAP];

    /// Pretend-descriptor pool. We hand out unique non-NULL pointers
    /// (the index of `STUB_DESCRIPTORS`) so the registry can store +
    /// retrieve them without aliasing the same value.
    static STUB_DESCRIPTORS: [u8; 64] = [0; 64];

    #[unsafe(no_mangle)]
    extern "C" fn nros_cyclonedds_build_descriptor_from_schema(
        _type_name: *const c_char,
        fields: *const NrosFieldDescriptor,
        field_count: u32,
        _kinds: *const NrosFieldKindDescriptor,
        _kind_count: u32,
        out_err: *mut c_int,
    ) -> *const c_void {
        let forced = FORCED_ERROR.swap(0, Ordering::SeqCst);
        if forced != 0 {
            if !out_err.is_null() {
                unsafe { *out_err = forced };
            }
            return core::ptr::null();
        }
        // Capture field (offset, kind) for inspection by tests.
        if !fields.is_null() {
            let cap = LAST_FIELDS_CAP.min(field_count as usize);
            LAST_FIELD_COUNT.store(cap, Ordering::SeqCst);
            for (i, slot) in LAST_FIELDS.iter().enumerate().take(cap) {
                // SAFETY: caller guarantees `fields` is valid for
                // `field_count` reads. Stub only writes into
                // `LAST_FIELDS` from one thread per call.
                let f = unsafe { &*fields.add(i) };
                unsafe {
                    *slot.0.get() = (f.offset, f.kind);
                }
            }
        }
        let idx = BUILD_COUNTER.fetch_add(1, Ordering::SeqCst);
        // Hand back a stable, unique non-NULL pointer per call (bounded
        // by the size of the stub backing array so we don't pretend to
        // exceed the test pool).
        let bounded = idx % STUB_DESCRIPTORS.len();
        let ptr: *const u8 = &STUB_DESCRIPTORS[bounded];
        ptr as *const c_void
    }

    #[unsafe(no_mangle)]
    extern "C" fn nros_rmw_cyclonedds_register_descriptor(
        _type_name: *const c_char,
        _descriptor: *const c_void,
    ) {
        // No-op: the C++ registry is irrelevant in unit tests; the
        // Rust registry already cached the pointer.
    }
}
