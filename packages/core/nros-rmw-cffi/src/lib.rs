//! C function table adapter for nros RMW backends.
//!
//! This crate provides a vtable-based bridge so that backends written in C,
//! C++, Zig, Ada, or any language with a C-compatible ABI can implement the
//! nros `Session` / `Publisher` / `Subscription` / service traits without
//! writing Rust code.
//!
//! # Usage (C backend implementor)
//!
//! 1. Include `<nros/rmw_vtable.h>`
//! 2. Implement all function pointers in `nros_rmw_vtable_t`
//! 3. Call `nros_rmw_cffi_register(&my_vtable)` before creating sessions
//!
//! # Usage (Rust consumer)
//!
//! Enable the `rmw-cffi` feature on `nros` and use `Executor<CffiSession>`.

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::{cell::UnsafeCell, ffi::c_void, sync::atomic::Ordering};

// RFC-0054 (phase-299 W1.3) — committed bindgen output from
// `packages/core/nros-rmw-abi/include/nros/*.h`. This module is the ONLY
// definition of the RMW ABI types (`nros_rmw_*_t`); the `NrosRmw*` names
// below are compat aliases over the generated items.
// (bindgen emits the vtable fn-pointer types inline, which trips
// clippy::type_complexity under `-D warnings`; allowed here rather than
// editing the generated file.)
#[allow(clippy::type_complexity)]
pub mod generated;
pub use generated::*;

use nros_rmw::{
    ClientTrait, MessageInfo, Publisher, QosDurabilityPolicy, QosHistoryPolicy,
    QosReliabilityPolicy, QosSettings, ServiceInfo, ServiceRequest, ServiceTrait, Session,
    TopicInfo, TransportError,
};

// Phase 115.L.0 — generic Rust→C-vtable adapter. Lives behind the
// `alloc` feature because each entity handle is boxed for stable
// address mgmt; every nros backend already requires alloc.
#[cfg(feature = "alloc")]
pub mod rust_adapter;

#[cfg(feature = "alloc")]
pub use rust_adapter::{RustBackend, RustBackendAdapter};

// Phase 249 P4b.1 — `.init_array` ctor self-registration
// (`nros_rmw_register_backend!` macro lives here).
pub mod section;

// ============================================================================
// Phase 102.1 / RFC-0054 — `nros_rmw_ret_t` named return codes
// ============================================================================
//
// The constants live in `generated` (from `<nros/rmw_ret.h>`); only the
// compat alias and the re-typed `OK` shadow live here.

/// Compat alias for the generated `nros_rmw_ret_t` typedef.
/// Zero on success; negative on error.
pub type NrosRmwRet = nros_rmw_ret_t;

// Anchor every C-stub-transport symbol so they survive
// `--gc-sections` when integration tests link against
// `libnros_rmw_cffi`. Only compiled when the c-stub-test feature
// is on; otherwise no C anchor + no toolchain dep.
#[cfg(feature = "c-stub-test")]
unsafe extern "C" {
    fn nros_c_stub_make_ops(out: *mut core::ffi::c_void);
    fn nros_c_stub_reset_counters();
    fn nros_c_stub_get_open_calls() -> u32;
    fn nros_c_stub_get_close_calls() -> u32;
    fn nros_c_stub_get_write_calls() -> u32;
    fn nros_c_stub_get_read_calls() -> u32;
}
#[cfg(feature = "c-stub-test")]
#[doc(hidden)]
pub fn _c_stub_transport_vtable_anchor() -> [*const core::ffi::c_void; 6] {
    [
        nros_c_stub_make_ops as *const _,
        nros_c_stub_reset_counters as *const _,
        nros_c_stub_get_open_calls as *const _,
        nros_c_stub_get_close_calls as *const _,
        nros_c_stub_get_write_calls as *const _,
        nros_c_stub_get_read_calls as *const _,
    ]
}
/// Map a `TransportError` to the corresponding `nros_rmw_ret_t` code.
///
/// By-reference because `TransportError` carries a `String` on its
/// dynamic-diagnostic variant and is not `Copy`. The string itself is
/// dropped at the boundary — embedded RMW callers cannot afford a
/// thread-local error buffer.
pub fn ret_from_error(err: &TransportError) -> NrosRmwRet {
    match err {
        TransportError::Timeout => NROS_RMW_RET_TIMEOUT,
        TransportError::WouldBlock => NROS_RMW_RET_WOULD_BLOCK,
        TransportError::TooLarge => NROS_RMW_RET_MESSAGE_TOO_LARGE,
        TransportError::BufferTooSmall => NROS_RMW_RET_BUFFER_TOO_SMALL,
        TransportError::MessageTooLarge => NROS_RMW_RET_MESSAGE_TOO_LARGE,
        TransportError::InvalidArgument => NROS_RMW_RET_INVALID_ARGUMENT,
        TransportError::InvalidConfig => NROS_RMW_RET_INVALID_ARGUMENT,
        TransportError::Unsupported => NROS_RMW_RET_UNSUPPORTED,
        TransportError::BadAlloc => NROS_RMW_RET_BAD_ALLOC,
        TransportError::IncompatibleQos => NROS_RMW_RET_INCOMPATIBLE_QOS,
        TransportError::TopicNameInvalid => NROS_RMW_RET_TOPIC_NAME_INVALID,
        TransportError::NodeNameNonExistent => NROS_RMW_RET_NODE_NAME_NON_EXISTENT,
        TransportError::LoanNotSupported => NROS_RMW_RET_LOAN_NOT_SUPPORTED,
        TransportError::NoData => NROS_RMW_RET_NO_DATA,
        TransportError::IncompatibleAbi => NROS_RMW_RET_INCOMPATIBLE_ABI,
        // Phase 155.B.3 — distinguish wire-level connection failure
        // from generic backend error so the FreeRTOS / RV64 C+C++
        // `init -> -X` logs identify the actual class. zenoh-pico's
        // `ZpicoError::Session` (zpico_open returned -3) and
        // `ZpicoError::Generic` (zpico_init returned -1) both flow
        // through `ZpicoError → ConnectionFailed`; the cmake-built
        // FreeRTOS C/C++ tests will now surface NOT_FOUND (the
        // user-side mapping in `nros_support_init`) instead of the
        // generic NROS_RET_ERROR catch-all.
        TransportError::ConnectionFailed | TransportError::Disconnected => {
            NROS_RMW_RET_CONNECTION_FAILED
        }
        // Everything else collapses to NROS_RMW_RET_ERROR. Backends
        // that want fine-grained reporting should adopt the named
        // variants above (Phase 102.2 sweep).
        _ => NROS_RMW_RET_ERROR,
    }
}

/// Map a `nros_rmw_ret_t` returned by a C-side vtable function back to
/// a `TransportError` for the Rust caller. Inverse of `ret_from_error`
/// — used when `nros-rmw-cffi`'s `CffiSession` etc. receive a code
/// from the registered C backend.
///
/// `NROS_RMW_RET_OK` is mapped to `TransportError::Backend("ok")` as a
/// programming-error sentinel; callers should branch on the success
/// path before calling this. Unknown negative codes collapse to the
/// generic `TransportError::Backend("unknown rmw_ret_t")` so a future
/// constant added to the C header degrades gracefully on the Rust side.
pub fn error_from_ret(ret: NrosRmwRet) -> TransportError {
    match ret {
        NROS_RMW_RET_OK => {
            TransportError::Backend("ok (logic error: positive ret_t at error site)")
        }
        NROS_RMW_RET_ERROR => TransportError::Backend("rmw_ret error"),
        NROS_RMW_RET_TIMEOUT => TransportError::Timeout,
        NROS_RMW_RET_BAD_ALLOC => TransportError::BadAlloc,
        NROS_RMW_RET_INVALID_ARGUMENT => TransportError::InvalidArgument,
        NROS_RMW_RET_UNSUPPORTED => TransportError::Unsupported,
        NROS_RMW_RET_INCOMPATIBLE_QOS => TransportError::IncompatibleQos,
        NROS_RMW_RET_TOPIC_NAME_INVALID => TransportError::TopicNameInvalid,
        NROS_RMW_RET_NODE_NAME_NON_EXISTENT => TransportError::NodeNameNonExistent,
        NROS_RMW_RET_LOAN_NOT_SUPPORTED => TransportError::LoanNotSupported,
        NROS_RMW_RET_NO_DATA => TransportError::NoData,
        NROS_RMW_RET_WOULD_BLOCK => TransportError::WouldBlock,
        NROS_RMW_RET_BUFFER_TOO_SMALL => TransportError::BufferTooSmall,
        NROS_RMW_RET_MESSAGE_TOO_LARGE => TransportError::MessageTooLarge,
        NROS_RMW_RET_INCOMPATIBLE_ABI => TransportError::IncompatibleAbi,
        // Phase 155.B.3 — inverse of `ret_from_error`'s
        // `ConnectionFailed | Disconnected → CONNECTION_FAILED`
        // mapping. Decodes the new vtable-level code back to the
        // `TransportError::ConnectionFailed` variant; downstream
        // `transport_error_to_ret` in nros-c surfaces it as
        // `NROS_RET_NOT_FOUND` (-4) to the user.
        NROS_RMW_RET_CONNECTION_FAILED => TransportError::ConnectionFailed,
        _ => TransportError::Backend("unknown rmw_ret_t"),
    }
}

// ============================================================================
// Phase 102.3 / RFC-0054 — typed entity structs (defined in `generated`)
// ============================================================================
//
// The `nros_rmw_*_t` structs live in `generated` (from
// `<nros/rmw_entity.h>`); the `NrosRmw*` names are compat aliases.

// The trait-level infinite spelling and the header's sentinel are the
// same value by contract; a header edit that drifts one fails here.
const _: () = assert!(nros_rmw::DURATION_INFINITE_MS as i64 == NROS_RMW_DURATION_INFINITE_MS);

/// Compat alias for the generated `nros_rmw_qos_t`.
pub type NrosRmwQos = nros_rmw_qos_t;
/// Compat alias for the generated `nros_rmw_session_t`.
pub type NrosRmwSession = nros_rmw_session_t;
/// Compat alias for the generated `nros_rmw_publisher_t`.
pub type NrosRmwPublisher = nros_rmw_publisher_t;
/// Compat alias for the generated `nros_rmw_subscription_t`.
pub type NrosRmwSubscription = nros_rmw_subscription_t;
/// Compat alias for the generated `nros_rmw_service_t`.
pub type NrosRmwService = nros_rmw_service_t;
/// Compat alias for the generated `nros_rmw_client_t`.
pub type NrosRmwClient = nros_rmw_client_t;
/// Compat alias for the generated `nros_rmw_vtable_t`.
pub type NrosRmwVtable = nros_rmw_vtable_t;

// The generated struct intentionally derives only Copy/Clone/Debug;
// consumers (and the hand-written predecessor) compare QoS profiles.
impl PartialEq for nros_rmw_qos_t {
    fn eq(&self, other: &Self) -> bool {
        self.reliability == other.reliability
            && self.durability == other.durability
            && self.history == other.history
            && self.liveliness_kind == other.liveliness_kind
            && self.depth == other.depth
            && self.deadline_ms == other.deadline_ms
            && self.lifespan_ms == other.lifespan_ms
            && self.liveliness_lease_ms == other.liveliness_lease_ms
            && self.avoid_ros_namespace_conventions == other.avoid_ros_namespace_conventions
    }
}
impl Eq for nros_rmw_qos_t {}

// The QoS profile constants below are `#define` struct-literal macros in the
// C header; bindgen does not translate function-like/struct-literal macros,
// so the Rust-side literals stay here (built from the generated types).

/// Standard `rmw_qos_profile_default`-equivalent.
pub const NROS_RMW_QOS_PROFILE_DEFAULT: NrosRmwQos = NrosRmwQos {
    reliability: 1, // RELIABLE
    durability: 0,  // VOLATILE
    history: 0,     // KEEP_LAST
    liveliness_kind: nros_rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_AUTOMATIC as u8,
    depth: 10,
    _reserved0: 0,
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    _reserved1: [0; 3],
};

/// Standard `rmw_qos_profile_sensor_data`-equivalent.
pub const NROS_RMW_QOS_PROFILE_SENSOR_DATA: NrosRmwQos = NrosRmwQos {
    reliability: 0, // BEST_EFFORT
    durability: 0,  // VOLATILE
    history: 0,     // KEEP_LAST
    liveliness_kind: nros_rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_AUTOMATIC as u8,
    depth: 5,
    _reserved0: 0,
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    _reserved1: [0; 3],
};

/// Standard `rmw_qos_profile_services_default`-equivalent.
pub const NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT: NrosRmwQos = NROS_RMW_QOS_PROFILE_DEFAULT;

/// Standard `rmw_qos_profile_parameters`-equivalent.
pub const NROS_RMW_QOS_PROFILE_PARAMETERS: NrosRmwQos = NrosRmwQos {
    depth: 1000,
    ..NROS_RMW_QOS_PROFILE_DEFAULT
};

/// Standard `rmw_qos_profile_system_default`-equivalent.
pub const NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT: NrosRmwQos = NROS_RMW_QOS_PROFILE_DEFAULT;

// Phase-301 (issue 0241) — the QoS lowering is FALLIBLE: a depth the
// C ABI's u16 cannot represent is a create-time error, never a silent
// saturate. The duration fields are u32 ms on both sides (0 = unset,
// `NROS_RMW_DURATION_INFINITE_MS` = explicit infinite) and pass
// through unchanged; finer-grained callers lower via
// `nros_rmw::duration_to_qos_ms` (sub-ms CEILs to 1 ms, past-u32
// errors).
impl TryFrom<QosSettings> for NrosRmwQos {
    type Error = TransportError;

    fn try_from(qos: QosSettings) -> Result<Self, TransportError> {
        if qos.depth > u16::MAX as u32 {
            return Err(TransportError::InvalidArgument);
        }
        Ok(Self {
            reliability: match qos.reliability {
                QosReliabilityPolicy::BestEffort => 0,
                QosReliabilityPolicy::Reliable => 1,
            },
            durability: match qos.durability {
                QosDurabilityPolicy::Volatile => 0,
                QosDurabilityPolicy::TransientLocal => 1,
            },
            history: match qos.history {
                QosHistoryPolicy::KeepLast => 0,
                QosHistoryPolicy::KeepAll => 1,
            },
            liveliness_kind: qos.liveliness_kind as u8,
            depth: qos.depth as u16,
            _reserved0: 0,
            deadline_ms: qos.deadline_ms,
            lifespan_ms: qos.lifespan_ms,
            liveliness_lease_ms: qos.liveliness_lease_ms,
            avoid_ros_namespace_conventions: qos.avoid_ros_namespace_conventions as u8,
            _reserved1: [0; 3],
        })
    }
}

// ============================================================================
// Phase 108 / RFC-0054 — status-event types (defined in `generated`)
// ============================================================================
//
// `nros_rmw_event_kind_t` is a module-consts alias (bindgen
// `--default-enum-style=moduleconsts`), not a Rust enum, so the retired
// `From` impls between it and `nros_rmw::EventKind` become plain functions.

/// Compat alias for the generated `nros_rmw_event_kind_t::Type`
/// (C-`unsigned`-sized event-kind discriminant).
pub type NrosRmwEventKind = nros_rmw_event_kind_t::Type;
/// Compat alias for the generated `nros_rmw_liveliness_changed_status_t`.
pub type NrosRmwLivelinessChangedStatus = nros_rmw_liveliness_changed_status_t;
/// Compat alias for the generated `nros_rmw_count_status_t`.
pub type NrosRmwCountStatus = nros_rmw_count_status_t;
/// Compat alias for the generated `nros_rmw_event_payload_t` union.
pub type NrosRmwEventPayload = nros_rmw_event_payload_t;
/// Compat alias for the generated `nros_rmw_event_callback_t`
/// (nullable — `Option`-wrapped fn pointer, per C ABI).
pub type NrosRmwEventCallback = nros_rmw_event_callback_t;

/// Convert a trait-level [`nros_rmw::EventKind`] to the C ABI discriminant.
/// Replaces the retired `From<nros_rmw::EventKind> for NrosRmwEventKind`.
pub fn event_kind_to_c(k: nros_rmw::EventKind) -> NrosRmwEventKind {
    use nros_rmw::EventKind as K;
    use nros_rmw_event_kind_t as C;
    match k {
        K::LivelinessChanged => C::NROS_RMW_EVENT_LIVELINESS_CHANGED,
        K::RequestedDeadlineMissed => C::NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED,
        K::MessageLost => C::NROS_RMW_EVENT_MESSAGE_LOST,
        K::LivelinessLost => C::NROS_RMW_EVENT_LIVELINESS_LOST,
        K::OfferedDeadlineMissed => C::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED,
        // unreachable for now (#[non_exhaustive])
        _ => C::NROS_RMW_EVENT_MESSAGE_LOST,
    }
}

/// Convert a C ABI event-kind discriminant to the trait-level
/// [`nros_rmw::EventKind`]. Replaces the retired
/// `From<NrosRmwEventKind> for nros_rmw::EventKind`. Unknown values map to
/// `MessageLost`, mirroring the forward direction's fallback.
pub fn event_kind_from_c(k: NrosRmwEventKind) -> nros_rmw::EventKind {
    use nros_rmw::EventKind as K;
    use nros_rmw_event_kind_t as C;
    match k {
        C::NROS_RMW_EVENT_LIVELINESS_CHANGED => K::LivelinessChanged,
        C::NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED => K::RequestedDeadlineMissed,
        C::NROS_RMW_EVENT_MESSAGE_LOST => K::MessageLost,
        C::NROS_RMW_EVENT_LIVELINESS_LOST => K::LivelinessLost,
        C::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED => K::OfferedDeadlineMissed,
        _ => K::MessageLost,
    }
}

// ============================================================================
// Registration
// ============================================================================
//
// Phase 104.B.2 — named registry replaces the singleton vtable.
// Backends register under a stable identifier (`"zenoh"`, `"dds"`,
// `"xrce"`, future `"uorb"`, `"cyclonedds"`); consumers look up
// vtables by name via `nros_rmw_cffi_lookup`. Multiple backends can
// coexist in the same process (bridge nodes).
//
// Capacity comes from the `NROS_RMW_MAX_BACKENDS` build-time env
// var (default 8). See `build.rs`.
//
// Implementation: a fixed-size `[BackendSlot; MAX_BACKENDS]`
// guarded by an atomic length counter. No alloc; `no_std`
// compatible. Slot scan is O(N) for lookup but N is tiny (8 by
// default). Each slot owns its name buffer; `name_ptr` returned
// to consumers points into the slot and stays valid for the
// program's lifetime.

/// Compile-time max number of concurrently registered backends.
/// Set via `NROS_RMW_MAX_BACKENDS` env var at build time
/// (`build.rs`). Default 8.
pub const MAX_BACKENDS: usize = parse_max_backends(env!("NROS_RMW_MAX_BACKENDS"));

const fn parse_max_backends(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut acc: usize = 0;
    while i < bytes.len() {
        let d = bytes[i];
        assert!(
            d.is_ascii_digit(),
            "NROS_RMW_MAX_BACKENDS must be a decimal integer"
        );
        acc = acc * 10 + (d - b'0') as usize;
        i += 1;
    }
    acc
}

/// Maximum length of a backend name. Names are short ASCII
/// identifiers (`"zenoh"`, `"cyclonedds"`); 32 bytes is generous.
const BACKEND_NAME_MAX: usize = 32;

#[repr(C)]
struct BackendSlot {
    /// Null-terminated UTF-8 backend name. Zero-initialized when
    /// unused (`name[0] == 0`).
    name: [u8; BACKEND_NAME_MAX],
    vtable: *const NrosRmwVtable,
}

impl BackendSlot {
    const fn empty() -> Self {
        Self {
            name: [0u8; BACKEND_NAME_MAX],
            vtable: core::ptr::null(),
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.name[0] == 0
    }

    #[inline]
    fn name_matches(&self, candidate: &[u8]) -> bool {
        if self.is_empty() {
            return false;
        }
        // Compare up to the first NUL or candidate length.
        let mut i = 0usize;
        while i < self.name.len() && i < candidate.len() {
            if self.name[i] == 0 {
                return false; // slot name shorter than candidate
            }
            if self.name[i] != candidate[i] {
                return false;
            }
            i += 1;
        }
        // candidate fully consumed; slot must be NUL at i (same length)
        i == candidate.len() && (i == self.name.len() || self.name[i] == 0)
    }
}

// SAFETY: `BackendSlot::vtable` is a `*const` pointer used in a
// `'static` context; once written it's never freed and the registry
// is guarded by an atomic length counter for publication. Marker
// trait implementations are required so the static array is
// `Sync` across threads.
unsafe impl Sync for BackendSlot {}

/// Fixed-size registry. `slots[0..len]` are live; `slots[len..]`
/// are zero-initialized. `len` is the publication fence.
///
/// `slots` lives in an `UnsafeCell` because we mutate through
/// `&'static REGISTRY`. Safety invariants:
/// * Slot writes happen only inside `nros_rmw_cffi_register_named`,
///   which is documented "call before `Executor::open`" — backend
///   ctors fire pre-main, manual calls precede session creation.
/// * Slot reads via `nros_rmw_cffi_lookup` and `get_vtable` happen
///   after `Executor::open`, well after registration completes.
/// * The atomic `len` provides the release-acquire fence so any
///   reader that sees `len = N` also sees the populated slot
///   contents for indices `< N`.
#[doc(hidden)]
pub struct Registry {
    slots: core::cell::UnsafeCell<[BackendSlot; MAX_BACKENDS]>,
    len: portable_atomic::AtomicUsize,
}

impl Registry {
    #[doc(hidden)]
    pub const fn new() -> Self {
        let slots = {
            #[allow(clippy::declare_interior_mutable_const)]
            const E: BackendSlot = BackendSlot::empty();
            [E; MAX_BACKENDS]
        };
        Self {
            slots: core::cell::UnsafeCell::new(slots),
            len: portable_atomic::AtomicUsize::new(0),
        }
    }

    /// Borrow slot `i` immutably. Caller must guarantee
    /// `i < self.len.load(Acquire)`.
    #[inline]
    unsafe fn slot(&self, i: usize) -> &BackendSlot {
        // SAFETY: registry protocol guarantees slot stability once
        // published via the atomic len fence.
        unsafe { &(*self.slots.get())[i] }
    }

    /// Borrow slot `i` mutably. Caller must guarantee exclusive
    /// access — either pre-publication (idx > current `len`) or
    /// during an idempotent overwrite of an already-registered name.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    unsafe fn slot_mut(&self, i: usize) -> &mut BackendSlot {
        // SAFETY: see Registry doc — writer-side discipline.
        unsafe { &mut (*self.slots.get())[i] }
    }
}

// SAFETY: see `Registry` doc-comment on the mutation protocol.
unsafe impl Sync for Registry {}

// Phase 241.D3-rev — `REGISTRY` is DEFINED once in this rlib (plain
// `#[no_mangle]`). The single-runtime model puts exactly one Rust staticlib in any
// link (the umbrella `nros-c` / `nros-cpp` bundles the backend as an rlib), so the
// cffi rlib appears once and one strong definition is correct everywhere: pure-Rust
// firmware, the NuttX build-std ELF, and the umbrella C/C++ staticlib alike. This
// supersedes the slice-4 `external-registry`/provider split, which existed only
// because the C/C++ link used to carry multiple Rust staticlibs.
#[unsafe(no_mangle)]
static REGISTRY: Registry = Registry::new();

/// The single process-wide backend registry.
#[inline]
fn registry() -> &'static Registry {
    &REGISTRY
}

// ============================================================================
// Rust-adapter MessageInfo side channel
// ============================================================================
//
// The stable C subscriber ABI returns only a `(payload, len)` pair from
// `try_recv_raw`. Rust backends can produce `MessageInfo`, so the generic
// Rust->C adapter stores that metadata keyed by the backend handle pointer
// immediately before returning the payload length. The Rust CFFI subscriber
// consumes it after the vtable call. Pure C/C++ backends never write this table
// and keep the documented `None` metadata behavior.

const MESSAGE_INFO_SLOTS: usize = 64;

struct MessageInfoSlot {
    key: portable_atomic::AtomicUsize,
    valid: portable_atomic::AtomicBool,
    info: UnsafeCell<MessageInfo>,
    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    validate_requested: portable_atomic::AtomicBool,
    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    integrity_valid: portable_atomic::AtomicBool,
    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    integrity: UnsafeCell<nros_rmw::IntegrityStatus>,
}

impl MessageInfoSlot {
    const fn empty() -> Self {
        Self {
            key: portable_atomic::AtomicUsize::new(0),
            valid: portable_atomic::AtomicBool::new(false),
            info: UnsafeCell::new(MessageInfo::new()),
            #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
            validate_requested: portable_atomic::AtomicBool::new(false),
            #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
            integrity_valid: portable_atomic::AtomicBool::new(false),
            #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
            integrity: UnsafeCell::new(nros_rmw::IntegrityStatus {
                gap: 0,
                duplicate: false,
                crc_valid: None,
            }),
        }
    }
}

// SAFETY: each slot is published by `key` and `valid` atomics. Writers store
// `info` before setting `valid = true` with Release ordering; readers take
// `valid` with AcqRel before copying the `MessageInfo`.
unsafe impl Sync for MessageInfoSlot {}

static MESSAGE_INFO_TABLE: [MessageInfoSlot; MESSAGE_INFO_SLOTS] = {
    #[allow(clippy::declare_interior_mutable_const)]
    const E: MessageInfoSlot = MessageInfoSlot::empty();
    [E; MESSAGE_INFO_SLOTS]
};

fn lookup_message_info_slot(key: usize) -> Option<&'static MessageInfoSlot> {
    if key == 0 {
        return None;
    }
    MESSAGE_INFO_TABLE
        .iter()
        .find(|slot| slot.key.load(Ordering::Acquire) == key)
}

#[cfg(feature = "alloc")]
fn get_or_insert_message_info_slot(key: usize) -> Option<&'static MessageInfoSlot> {
    if key == 0 {
        return None;
    }
    for slot in &MESSAGE_INFO_TABLE {
        let current = slot.key.load(Ordering::Acquire);
        if current == key {
            return Some(slot);
        }
        if current == 0
            && slot
                .key
                .compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return Some(slot);
        }
    }
    None
}

#[cfg(feature = "alloc")]
pub(crate) fn store_cffi_message_info(key: usize, info: Option<MessageInfo>) {
    let Some(slot) = get_or_insert_message_info_slot(key) else {
        return;
    };
    match info {
        Some(info) => {
            // SAFETY: this slot is keyed to one subscriber backend handle. The
            // executor owns each subscriber mutably while receiving, so writes
            // for the same key are serialized.
            unsafe {
                *slot.info.get() = info;
            }
            slot.valid.store(true, Ordering::Release);
        }
        None => slot.valid.store(false, Ordering::Release),
    }
}

fn take_cffi_message_info(key: usize) -> Option<MessageInfo> {
    let slot = lookup_message_info_slot(key)?;
    if !slot.valid.swap(false, Ordering::AcqRel) {
        return None;
    }
    // SAFETY: `valid.swap(false)` gives this reader exclusive consumption of the
    // last stored `MessageInfo` for this key.
    Some(unsafe { *slot.info.get() })
}

#[cfg(all(feature = "alloc", feature = "safety-e2e"))]
fn request_cffi_integrity_status(key: usize) {
    let Some(slot) = get_or_insert_message_info_slot(key) else {
        return;
    };
    slot.integrity_valid.store(false, Ordering::Release);
    slot.validate_requested.store(true, Ordering::Release);
}

#[cfg(all(feature = "alloc", feature = "safety-e2e"))]
pub(crate) fn take_cffi_integrity_request(key: usize) -> bool {
    lookup_message_info_slot(key)
        .map(|slot| slot.validate_requested.swap(false, Ordering::AcqRel))
        .unwrap_or(false)
}

#[cfg(all(feature = "alloc", feature = "safety-e2e"))]
pub(crate) fn store_cffi_integrity_status(key: usize, status: nros_rmw::IntegrityStatus) {
    let Some(slot) = get_or_insert_message_info_slot(key) else {
        return;
    };
    // SAFETY: integrity status follows the same per-subscriber handoff as
    // `info`; the CFFI subscriber owns receive calls mutably for this key.
    unsafe {
        *slot.integrity.get() = status;
    }
    slot.integrity_valid.store(true, Ordering::Release);
}

#[cfg(all(feature = "alloc", feature = "safety-e2e"))]
fn take_cffi_integrity_status(key: usize) -> Option<nros_rmw::IntegrityStatus> {
    let slot = lookup_message_info_slot(key)?;
    if !slot.integrity_valid.swap(false, Ordering::AcqRel) {
        return None;
    }
    Some(unsafe { *slot.integrity.get() })
}

fn clear_cffi_message_info(key: usize) {
    let Some(slot) = lookup_message_info_slot(key) else {
        return;
    };
    slot.valid.store(false, Ordering::Release);
    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    {
        slot.validate_requested.store(false, Ordering::Release);
        slot.integrity_valid.store(false, Ordering::Release);
    }
    slot.key.store(0, Ordering::Release);
}

/// Register a custom RMW backend vtable (legacy single-arg form).
///
/// Phase 104.B.2 — internally forwards to
/// [`nros_rmw_cffi_register_named`] with the literal name `"default"`.
/// Preserved as a one-release source-compat shim so backend ctors
/// authored before the named-registry switchover keep working.
///
/// **Deprecated (Phase 128.B.5).** All in-tree callers now use
/// [`nros_rmw_cffi_register_named`] directly so the registry slot is
/// keyed by the backend's canonical name (`"zenoh"`, `"dds"`,
/// `"xrce"`, `"cyclonedds"`, …). New backends MUST follow the same
/// pattern; the unnamed shim will be removed in a follow-up phase
/// once external callers have migrated.
///
/// # Safety
///
/// The vtable pointer must remain valid for the lifetime of the program.
/// All function pointers in the vtable must be valid.
#[deprecated(
    since = "0.2.0",
    note = "use nros_rmw_cffi_register_named with the backend's canonical name; the unnamed shim will be removed"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_register(vtable: *const NrosRmwVtable) -> NrosRmwRet {
    unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), vtable) }
}

/// Register a backend under a stable name. Multiple backends can
/// coexist; consumers select via [`nros_rmw_cffi_lookup`] or the
/// higher-level `Executor::node_builder(...).rmw(...)` path.
///
/// Names must be UTF-8, NUL-terminated, ≤ 31 bytes (excluding NUL).
/// Reserved names today: `"zenoh"`, `"dds"`, `"xrce"`,
/// `"cyclonedds"`, future `"uorb"`. The string `"default"` is the
/// implicit name used by the legacy single-arg
/// [`nros_rmw_cffi_register`] shim.
///
/// Returns:
/// * `NROS_RMW_RET_OK` on success.
/// * `NROS_RMW_RET_INVALID_ARGUMENT` if `name` / `vtable` is
///   NULL, the name is empty, or exceeds 31 bytes.
/// * `NROS_RMW_RET_ERROR` if the registry is full
///   (`MAX_BACKENDS` reached without a matching entry).
///
/// Duplicate registration of the same name overwrites the
/// previous vtable (idempotent for ctor-fires-twice cases).
///
/// # Safety
///
/// * `name` must be a valid NUL-terminated UTF-8 string.
/// * `vtable` must remain valid for the program's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_register_named(
    name: *const core::ffi::c_char,
    vtable: *const NrosRmwVtable,
) -> NrosRmwRet {
    if name.is_null() || vtable.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let name_u8 = name.cast::<u8>();

    // Length-check the input. We scan up to BACKEND_NAME_MAX + 1
    // bytes; anything longer is rejected.
    let mut len = 0usize;
    while len < BACKEND_NAME_MAX {
        let b = unsafe { *name_u8.add(len) };
        if b == 0 {
            break;
        }
        len += 1;
    }
    if len == 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Must have found a NUL within BACKEND_NAME_MAX.
    if unsafe { *name_u8.add(len) } != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    let name_bytes = unsafe { core::slice::from_raw_parts(name_u8, len) };

    // First pass: look for existing entry with same name → overwrite.
    let current_len = registry().len.load(Ordering::Acquire);
    for i in 0..current_len {
        // SAFETY: i < current_len, indices in bounds.
        let slot = unsafe { registry().slot(i) };
        if slot.name_matches(name_bytes) {
            // SAFETY: writer-side idempotent overwrite. The slot is
            // already published; concurrent readers will see either
            // the old or new vtable consistently, both valid.
            unsafe {
                let slot_mut = registry().slot_mut(i);
                slot_mut.vtable = vtable;
            }
            core::sync::atomic::fence(Ordering::Release);
            return NROS_RMW_RET_OK;
        }
    }

    // No existing entry; append. Reserve a slot via atomic increment.
    let idx = registry().len.fetch_add(1, Ordering::AcqRel);
    if idx >= MAX_BACKENDS {
        // Roll back the increment so subsequent registers don't see a
        // stale `len > MAX_BACKENDS`. (Race window negligible — once
        // we hit capacity, no further append succeeds.)
        registry().len.store(MAX_BACKENDS, Ordering::Release);
        return NROS_RMW_RET_ERROR;
    }

    // SAFETY: idx < MAX_BACKENDS, mutating an as-yet-unpublished slot.
    unsafe {
        let slot = registry().slot_mut(idx);
        slot.name[..len].copy_from_slice(name_bytes);
        slot.name[len] = 0;
        slot.vtable = vtable;
    }
    // Release-fence so concurrent lookups see both the name and the
    // vtable consistently with the updated `len`.
    core::sync::atomic::fence(Ordering::Release);
    NROS_RMW_RET_OK
}

/// Look up a backend's vtable by name. Returns NULL if no backend
/// is registered under `name`.
///
/// # Safety
///
/// * `name` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_lookup(
    name: *const core::ffi::c_char,
) -> *const NrosRmwVtable {
    if name.is_null() {
        return core::ptr::null();
    }
    let name_u8 = name.cast::<u8>();
    let mut len = 0usize;
    while len < BACKEND_NAME_MAX {
        if unsafe { *name_u8.add(len) } == 0 {
            break;
        }
        len += 1;
    }
    if len == 0 || len == BACKEND_NAME_MAX {
        return core::ptr::null();
    }
    let name_bytes = unsafe { core::slice::from_raw_parts(name_u8, len) };

    let current_len = registry().len.load(Ordering::Acquire);
    for i in 0..current_len {
        // SAFETY: i < current_len, indices in bounds; publication
        // fence via the atomic-len Acquire load.
        let slot = unsafe { registry().slot(i) };
        if slot.name_matches(name_bytes) {
            return slot.vtable;
        }
    }
    core::ptr::null()
}

/// Diagnostic helper — fills `buf` with pointers to up to `cap`
/// registered backend names. Returns the number of names available
/// (may exceed `cap`). Pointer-valid for the program's lifetime.
///
/// # Safety
///
/// * `buf` must either be NULL (when `cap == 0`) or point at writable
///   memory of at least `cap * sizeof(*const c_char)` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_registered_names(
    buf: *mut *const core::ffi::c_char,
    cap: usize,
) -> usize {
    let n = registry().len.load(Ordering::Acquire);
    if !buf.is_null() && cap > 0 {
        let limit = n.min(cap);
        for i in 0..limit {
            // SAFETY: i < limit <= cap, buf capacity guaranteed by caller.
            let slot = unsafe { registry().slot(i) };
            unsafe {
                buf.add(i)
                    .write(slot.name.as_ptr() as *const core::ffi::c_char)
            };
        }
    }
    n
}

/// Phase 104.A — registry-presence probe. Returns `true` iff at
/// least one backend is registered. Used by `Executor::open` to
/// detect "user forgot to register a backend before opening the
/// session" and fail with a meaningful error.
#[inline]
pub fn backend_registered() -> bool {
    registry().len.load(Ordering::Acquire) > 0
}

/// Phase 104.B — internal access to the registry for the Rust-side
/// adapter. `nros-node`'s `register_active_backend` removal already
/// switched to `backend_registered()` for the presence check; this
/// returns the vtable for any single-backend fast-path callers.
fn default_vtable() -> Option<&'static NrosRmwVtable> {
    let n = registry().len.load(Ordering::Acquire);
    if n == 0 {
        return None;
    }
    // SAFETY: index 0 < n, registry's len-Acquire fence orders the
    // slot read.
    let slot = unsafe { registry().slot(0) };
    if slot.vtable.is_null() {
        return None;
    }
    Some(unsafe { &*slot.vtable })
}

/// Phase 128.A.3 — outcome of `resolve_backend`.
pub enum BackendResolution {
    /// Exactly one matching backend; use its vtable.
    Single(&'static NrosRmwVtable),
    /// No backend linked into the binary. Maps to
    /// [`NROS_RMW_RET_NO_BACKEND`].
    NoBackend,
    /// More than one backend linked and no selector given. Maps to
    /// [`NROS_RMW_RET_AMBIGUOUS_BACKEND`].
    Ambiguous,
    /// Selector did not match any registered backend. Maps to
    /// [`NROS_RMW_RET_UNKNOWN_BACKEND`].
    Unknown,
}

/// Phase 128.A.3 — selection policy for the single-backend
/// `Executor::open` / `nros::init` path.
///
/// Algorithm:
///
/// 1. If `selector` is `Some(name)` (typically from `$NROS_RMW`),
///    look it up in the registry. Hit → [`BackendResolution::Single`];
///    miss → [`BackendResolution::Unknown`].
/// 2. Otherwise, if exactly one backend is registered, return it.
/// 3. Otherwise, if zero, [`BackendResolution::NoBackend`]; if more
///    than one, [`BackendResolution::Ambiguous`].
///
/// Callers convert the resolution to a [`NrosRmwRet`] via
/// [`backend_resolution_to_ret`].
///
/// Bridge consumers (`Executor::open_multi`) bypass this function and
/// call `nros_rmw_cffi_lookup` per spec instead.
pub fn resolve_backend(selector: Option<&[u8]>) -> BackendResolution {
    let n = registry().len.load(Ordering::Acquire);
    if let Some(name) = selector {
        let mut i = 0usize;
        while i < n {
            // SAFETY: i < n, registry len-Acquire fence orders the read.
            let slot = unsafe { registry().slot(i) };
            if slot.name_matches(name) {
                if slot.vtable.is_null() {
                    return BackendResolution::Unknown;
                }
                return BackendResolution::Single(unsafe { &*slot.vtable });
            }
            i += 1;
        }
        return BackendResolution::Unknown;
    }
    match n {
        0 => BackendResolution::NoBackend,
        1 => default_vtable()
            .map(BackendResolution::Single)
            .unwrap_or(BackendResolution::NoBackend),
        _ => BackendResolution::Ambiguous,
    }
}

/// Phase 128.A.3 — map a [`BackendResolution`] to its canonical
/// [`NrosRmwRet`]. [`BackendResolution::Single`] is *not* an error and
/// returns [`NROS_RMW_RET_OK`]; callers needing the vtable should
/// pattern-match on the resolution itself.
pub fn backend_resolution_to_ret(res: &BackendResolution) -> NrosRmwRet {
    match res {
        BackendResolution::Single(_) => NROS_RMW_RET_OK,
        BackendResolution::NoBackend => NROS_RMW_RET_NO_BACKEND,
        BackendResolution::Ambiguous => NROS_RMW_RET_AMBIGUOUS_BACKEND,
        BackendResolution::Unknown => NROS_RMW_RET_UNKNOWN_BACKEND,
    }
}

/// Phase 115.A.2 — C entry point for installing a custom transport.
///
/// Mirrors the Rust-side `nros_rmw::set_custom_transport(Some(...))`
/// (or `None` when `ops == NULL`) but returns the canonical
/// `nros_rmw_ret_t` codes so non-Rust consumers don't have to
/// reach into nros-c's higher-level error enum.
///
/// The struct's contents are copied internally; the caller may
/// stack-allocate. Pass `NULL` to clear the slot.
///
/// # Safety
///
/// `ops` must either be `NULL` or point at a valid
/// `nros_transport_ops_t` whose four fn pointers stay live for the
/// lifetime of the registration (i.e. until a subsequent
/// `nros_rmw_cffi_set_custom_transport(NULL)` or a replacement
/// install).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_set_custom_transport(
    ops: *const nros_rmw::NrosTransportOps,
) -> NrosRmwRet {
    if ops.is_null() {
        // Clear: ignore any error (None is always accepted).
        let _ = unsafe { nros_rmw::set_custom_transport(None) };
        return NROS_RMW_RET_OK;
    }
    let copy = unsafe { *ops };
    match unsafe { nros_rmw::set_custom_transport(Some(copy)) } {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

fn get_vtable() -> Result<&'static NrosRmwVtable, TransportError> {
    // Phase 104.B.2 — fast path: registry has exactly one backend.
    // Mirror the single-backend hot path the singleton-VTABLE
    // implementation had. Bridge / multi-backend users should call
    // a forthcoming `get_vtable_named` API (104.C work) instead.
    default_vtable().ok_or(TransportError::InvalidArgument)
}

// ============================================================================
// Helper: null-terminated string on the stack
// ============================================================================

/// Write a Rust `&str` as a null-terminated byte sequence into a fixed buffer.
/// Returns a pointer to the buffer start (as C `char*`, matching the
/// generated ABI's string parameters).
fn to_c_str<const N: usize>(s: &str, buf: &mut [u8; N]) -> *const core::ffi::c_char {
    let len = s.len().min(N - 1);
    buf[..len].copy_from_slice(&s.as_bytes()[..len]);
    buf[len] = 0;
    buf.as_ptr().cast()
}

/// Inverse of [`to_c_str`] — read a null-terminated byte buffer back
/// as a `&str`, stopping at the first NUL byte. Used by the
/// `topic_name()` / `type_name()` / `node_name()` accessors on the
/// `Cffi*` types so callers can introspect without round-tripping
/// through the vtable. Phase 102.5.
fn cstr_buf_to_str<const N: usize>(buf: &[u8; N]) -> &str {
    let len = buf.iter().position(|&b| b == 0).unwrap_or(N);
    // The buffers are written via `to_c_str` from a `&str`, so the
    // bytes between [..len] are guaranteed valid UTF-8. `from_utf8`
    // handles the (impossible) corruption case by returning empty.
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

// ============================================================================
// CffiSession
// ============================================================================
//
// Storage discipline:
// * Each Cffi* struct owns null-terminated name buffers as inline
//   arrays. The C-side typed entity struct is rebuilt fresh on every
//   FFI call via `make_*_view`, so move-invalidation of pointers
//   into the buffer is impossible — the pointer always points to the
//   *current* address of the buffer, computed at call time.
// * The backend writes `backend_data` (and `can_loan_messages` for
//   pub/sub entities)
//   into the FFI view; we copy the writes back into the Cffi*
//   struct's fields after the call.
// * Strings ARE immutable for the entity's lifetime, so backends that
//   stash the topic_name pointer for diagnostics see stable storage
//   *as long as the Cffi* struct is not moved.* The Phase 102.4
//   contract is "do not move a Cffi* struct after construction" —
//   nano-ros embeds them inside the executor arena, which doesn't
//   relocate.

const NAME_BUF_LEN: usize = 256;
const HASH_BUF_LEN: usize = 128;

/// Session backed by a C vtable.
pub struct CffiSession {
    vtable: &'static NrosRmwVtable,
    /// Borrowed-pointer storage for `node_name`. Outlives the session.
    node_name_buf: [u8; NAME_BUF_LEN],
    /// Borrowed-pointer storage for `namespace_`. Empty for now —
    /// `RmwConfig` does not yet carry a namespace through the cffi
    /// path; reserved for future use.
    namespace_buf: [u8; NAME_BUF_LEN],
    /// Backend-private state, written by `vtable.create_session`.
    backend_data: *mut c_void,
}

impl CffiSession {
    fn make_view(&mut self) -> NrosRmwSession {
        NrosRmwSession {
            node_name: self.node_name_buf.as_ptr().cast(),
            namespace_: self.namespace_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        }
    }

    /// Phase 268 — build a per-call session view whose `node_name` / `namespace_`
    /// carry the ENTITY's owning-node identity (when the entity declares one),
    /// not the session's open-time default.
    ///
    /// A backend reads `session->node_name` to tag the entity it is creating for
    /// ROS 2 graph discovery (`ros2 node list`). One session can host N graph
    /// nodes (e.g. a multi-node launch entry), so the session's single open-time
    /// name is wrong for any entity owned by a different node. #104 threaded the
    /// node name only into the session, so multi-node entries collapsed every
    /// entity onto the one session name (`/node`). Overriding per entity here is
    /// the fix — no vtable ABI / signature change, every backend benefits (it
    /// already reads `session->node_name`).
    ///
    /// Falls back to the session buffers when the entity carries no node identity
    /// (direct-API / single-node path) — backward-compatible. The staging buffers
    /// must outlive the synchronous trampoline call; callers keep them on the
    /// stack across the `(vtable.create_*)` call.
    fn entity_view(
        &self,
        node_name: Option<&str>,
        namespace: &str,
        nn_buf: &mut [u8; NAME_BUF_LEN],
        ns_buf: &mut [u8; NAME_BUF_LEN],
    ) -> NrosRmwSession {
        let node_name_ptr = match node_name {
            Some(n) if !n.is_empty() => to_c_str(n, nn_buf),
            _ => self.node_name_buf.as_ptr().cast(),
        };
        let namespace_ptr = if namespace.is_empty() {
            self.namespace_buf.as_ptr().cast()
        } else {
            to_c_str(namespace, ns_buf)
        };
        NrosRmwSession {
            node_name: node_name_ptr,
            namespace_: namespace_ptr,
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        }
    }

    /// Node name passed at session-open time.
    pub fn node_name(&self) -> &str {
        cstr_buf_to_str(&self.node_name_buf)
    }

    /// Open a new session via the **default** registered vtable
    /// (first entry in the registry — the RMW_IMPLEMENTATION-style
    /// fast path for single-backend builds).
    ///
    /// For explicit backend selection in multi-backend (bridge)
    /// binaries, use [`open_named`](Self::open_named).
    pub fn open(
        locator: &str,
        mode: u8,
        domain_id: u32,
        node_name: &str,
    ) -> Result<Self, TransportError> {
        let vtable = get_vtable()?;
        Self::open_with_vtable(vtable, locator, mode, domain_id, node_name)
    }

    /// Phase 104.C.1 — open a new session against a named backend.
    /// Resolves `rmw_name` against the registry (Phase 104.B.2),
    /// returns `Err(TransportError::InvalidArgument)` if no backend
    /// is registered under that name.
    pub fn open_named(
        rmw_name: &str,
        locator: &str,
        mode: u8,
        domain_id: u32,
        node_name: &str,
    ) -> Result<Self, TransportError> {
        // C-string-marshal `rmw_name` on the stack — registry lookup
        // expects NUL-terminated UTF-8.
        let mut name_buf = [0u8; BACKEND_NAME_MAX];
        if rmw_name.len() >= BACKEND_NAME_MAX {
            return Err(TransportError::InvalidArgument);
        }
        name_buf[..rmw_name.len()].copy_from_slice(rmw_name.as_bytes());
        // name_buf[rmw_name.len()] is already 0.
        let raw = unsafe { nros_rmw_cffi_lookup(name_buf.as_ptr() as *const _) };
        if raw.is_null() {
            return Err(TransportError::InvalidArgument);
        }
        // SAFETY: registry-issued pointer; valid for the program's lifetime.
        let vtable = unsafe { &*raw };
        Self::open_with_vtable(vtable, locator, mode, domain_id, node_name)
    }

    fn open_with_vtable(
        vtable: &'static NrosRmwVtable,
        locator: &str,
        mode: u8,
        domain_id: u32,
        node_name: &str,
    ) -> Result<Self, TransportError> {
        let mut loc_buf = [0u8; NAME_BUF_LEN];
        let loc_ptr = to_c_str(locator, &mut loc_buf);

        let mut session = Self {
            vtable,
            node_name_buf: [0u8; NAME_BUF_LEN],
            namespace_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::null_mut(),
        };
        let _ = to_c_str(node_name, &mut session.node_name_buf);

        let mut view = NrosRmwSession {
            node_name: session.node_name_buf.as_ptr().cast(),
            namespace_: session.namespace_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        let ret = unsafe {
            (vtable.create_session.expect("rmw vtable: create_session"))(
                loc_ptr,
                mode,
                domain_id,
                session.node_name_buf.as_ptr().cast(),
                &mut view,
            )
        };
        // Phase 156.4 — diagnostic for bridge runtime
        // ConnectionFailed investigation. Logs the raw ret +
        // post-open backend_data state so callers see which of
        // the two failure paths fired. Gated on env var so
        // production traffic stays quiet.
        #[cfg(feature = "std")]
        if std::env::var_os("NROS_RMW_TRACE_OPEN").is_some() {
            std::eprintln!(
                "[nros-rmw-cffi] open: locator={locator:?} mode={mode} ret={ret} backend_data={:p}",
                view.backend_data,
            );
        }
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if view.backend_data.is_null() {
            return Err(TransportError::ConnectionFailed);
        }
        session.backend_data = view.backend_data;
        Ok(session)
    }
}

impl Session for CffiSession {
    type Error = TransportError;
    type PublisherHandle = CffiPublisher;
    type SubscriptionHandle = CffiSubscription;
    type ServiceHandle = CffiService;
    type ClientHandle = CffiClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiPublisher, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let qos_struct = NrosRmwQos::try_from(qos)?;
        // phase-301 (issue 0240) — the express hint travels in the options
        // struct, not the QoS profile. Either surface wins: the QoS profile
        // field (language APIs) or `TopicInfo::with_tx_express` (direct RMW).
        let options = nros_rmw_publisher_options_t {
            tx_express: (topic.tx_express || qos.tx_express) as u8,
            _reserved: [0u8; 7],
        };

        let mut pub_state = CffiPublisher {
            vtable: self.vtable,
            topic_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            qos: qos_struct,
            can_loan_messages: false,
            backend_data: core::ptr::null_mut(),
        };
        let topic_ptr = to_c_str(topic.name, &mut pub_state.topic_name_buf);
        let type_ptr = to_c_str(topic.type_name, &mut pub_state.type_name_buf);

        let mut view = NrosRmwPublisher {
            topic_name: topic_ptr,
            type_name: type_ptr,
            qos: qos_struct,
            can_loan_messages: false,
            _reserved: [0u8; 7],
            backend_data: core::ptr::null_mut(),
        };
        // Phase 268 — tag the entity with its owning node, not the session default.
        let mut nn_buf = [0u8; NAME_BUF_LEN];
        let mut ns_buf = [0u8; NAME_BUF_LEN];
        let mut session_view =
            self.entity_view(topic.node_name, topic.namespace, &mut nn_buf, &mut ns_buf);
        let ret = unsafe {
            (self
                .vtable
                .create_publisher
                .expect("rmw vtable: create_publisher"))(
                &mut session_view,
                topic_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &qos_struct,
                &options,
                &mut view,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if view.backend_data.is_null() {
            return Err(TransportError::PublisherCreationFailed);
        }
        pub_state.backend_data = view.backend_data;
        pub_state.can_loan_messages = view.can_loan_messages;
        Ok(pub_state)
    }

    fn create_subscription(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiSubscription, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let qos_struct = NrosRmwQos::try_from(qos)?;
        // Phase 231 (RFC-0038) / phase-301 (issue 0240) — the receive-buffer
        // size hint travels in the options struct so a size-classing backend
        // can route its receive storage. A hint, not a policy: oversize
        // saturates.
        let options = nros_rmw_subscription_options_t {
            rx_buffer_hint: topic.rx_buffer_hint.min(u32::MAX as usize) as u32,
            _reserved: [0u8; 4],
        };

        let mut sub_state = CffiSubscription {
            vtable: self.vtable,
            topic_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            qos: qos_struct,
            can_loan_messages: false,
            backend_data: core::ptr::null_mut(),
            supports_in_place: false,
        };
        let topic_ptr = to_c_str(topic.name, &mut sub_state.topic_name_buf);
        let type_ptr = to_c_str(topic.type_name, &mut sub_state.type_name_buf);

        let mut view = NrosRmwSubscription {
            topic_name: topic_ptr,
            type_name: type_ptr,
            qos: qos_struct,
            can_loan_messages: false,
            _reserved: [0u8; 7],
            backend_data: core::ptr::null_mut(),
        };
        // Phase 268 — tag the entity with its owning node, not the session default.
        let mut nn_buf = [0u8; NAME_BUF_LEN];
        let mut ns_buf = [0u8; NAME_BUF_LEN];
        let mut session_view =
            self.entity_view(topic.node_name, topic.namespace, &mut nn_buf, &mut ns_buf);
        let ret = unsafe {
            (self
                .vtable
                .create_subscription
                .expect("rmw vtable: create_subscription"))(
                &mut session_view,
                topic_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &qos_struct,
                &options,
                &mut view,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if view.backend_data.is_null() {
            return Err(TransportError::SubscriberCreationFailed);
        }
        sub_state.backend_data = view.backend_data;
        sub_state.can_loan_messages = view.can_loan_messages;
        // Phase 231 (RFC-0038) — cache the in-place capability once.
        sub_state.supports_in_place = match sub_state.vtable.subscription_supports_in_place {
            Some(f) => {
                let mut v = sub_state.make_view();
                unsafe { f(&mut v) == 1 }
            }
            None => false,
        };
        Ok(sub_state)
    }

    fn create_service(
        &mut self,
        service: &ServiceInfo,
        qos: QosSettings,
    ) -> Result<CffiService, TransportError> {
        let qos_struct = NrosRmwQos::try_from(qos)?;
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let mut srv_state = CffiService {
            vtable: self.vtable,
            service_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::null_mut(),
        };
        let svc_ptr = to_c_str(service.name, &mut srv_state.service_name_buf);
        let type_ptr = to_c_str(service.type_name, &mut srv_state.type_name_buf);

        let mut view = NrosRmwService {
            service_name: svc_ptr,
            type_name: type_ptr,
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        // Phase 268 — tag the entity with its owning node, not the session default.
        let mut nn_buf = [0u8; NAME_BUF_LEN];
        let mut ns_buf = [0u8; NAME_BUF_LEN];
        let mut session_view = self.entity_view(
            service.node_name,
            service.namespace,
            &mut nn_buf,
            &mut ns_buf,
        );
        let ret = unsafe {
            (self
                .vtable
                .create_service
                .expect("rmw vtable: create_service"))(
                &mut session_view,
                svc_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
                &qos_struct,
                &mut view,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if view.backend_data.is_null() {
            return Err(TransportError::ServiceServerCreationFailed);
        }
        srv_state.backend_data = view.backend_data;
        Ok(srv_state)
    }

    fn create_client(
        &mut self,
        service: &ServiceInfo,
        qos: QosSettings,
    ) -> Result<CffiClient, TransportError> {
        let qos_struct = NrosRmwQos::try_from(qos)?;
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let mut cli_state = CffiClient {
            vtable: self.vtable,
            service_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::null_mut(),
        };
        let svc_ptr = to_c_str(service.name, &mut cli_state.service_name_buf);
        let type_ptr = to_c_str(service.type_name, &mut cli_state.type_name_buf);

        let mut view = NrosRmwClient {
            service_name: svc_ptr,
            type_name: type_ptr,
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        // Phase 268 — tag the entity with its owning node, not the session default.
        let mut nn_buf = [0u8; NAME_BUF_LEN];
        let mut ns_buf = [0u8; NAME_BUF_LEN];
        let mut session_view = self.entity_view(
            service.node_name,
            service.namespace,
            &mut nn_buf,
            &mut ns_buf,
        );
        let ret = unsafe {
            (self
                .vtable
                .create_client
                .expect("rmw vtable: create_client"))(
                &mut session_view,
                svc_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
                &qos_struct,
                &mut view,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if view.backend_data.is_null() {
            return Err(TransportError::ServiceClientCreationFailed);
        }
        cli_state.backend_data = view.backend_data;
        Ok(cli_state)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if self.backend_data.is_null() {
            return Ok(());
        }
        let mut view = self.make_view();
        let ret = unsafe {
            (self
                .vtable
                .destroy_session
                .expect("rmw vtable: destroy_session"))(&mut view)
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        self.backend_data = core::ptr::null_mut();
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        let mut view = self.make_view();
        let ret =
            unsafe { (self.vtable.drive_io.expect("rmw vtable: drive_io"))(&mut view, timeout_ms) };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }

    fn next_deadline_ms(&self) -> Option<u32> {
        let f = self.vtable.next_deadline_ms?;
        // SAFETY: build a transient `&self`-only view of the session
        // fields the C side may inspect; matches the layout `make_view`
        // produces but doesn't require `&mut self`.
        let view = NrosRmwSession {
            node_name: self.node_name_buf.as_ptr().cast(),
            namespace_: self.namespace_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        };
        let ret = unsafe { f(&view as *const _) };
        if ret < 0 { None } else { Some(ret as u32) }
    }

    unsafe fn set_wake_callback(
        &mut self,
        cb: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void)>,
        ctx: *mut core::ffi::c_void,
    ) {
        let Some(f) = self.vtable.set_wake_callback else {
            return;
        };
        let mut view = NrosRmwSession {
            node_name: self.node_name_buf.as_ptr().cast(),
            namespace_: self.namespace_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        };
        // SAFETY: vtable trampoline owns the install/clear; result is
        // ignored — best-effort.
        let _ = unsafe { f(&mut view as *mut _, cb, ctx) };
    }

    fn supports_wake_callback(&self) -> bool {
        // Phase 130.4 — the vtable slot's presence is the truthful
        // signal. Poll-only backends (XRCE-DDS-Client, current
        // Cyclone wrapper, current dust-DDS shim) leave the slot
        // NULL; only backends with an async wake source fill it.
        self.vtable.set_wake_callback.is_some()
    }

    fn ping_session(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        // Phase 124.F.1 — forward to the backend's vtable slot when
        // available; NULL surfaces `Unsupported` to the caller (no
        // implicit emulation — backends without a wire-level
        // round-trip can't probe honestly).
        let Some(f) = self.vtable.ping_session else {
            return Err(TransportError::Unsupported);
        };
        let mut view = self.make_view();
        let rc = unsafe { f(&mut view, timeout_ms) };
        if rc == NROS_RMW_RET_OK {
            Ok(())
        } else {
            Err(error_from_ret(rc))
        }
    }

    /// Phase 115.K.2.5.1.2 — declare a permissive QoS-policy mask
    /// here so backends behind the cffi vtable don't get rejected by
    /// the runtime's pre-validate step before they ever see the
    /// `create_publisher` / `create_subscription` call. The vtable
    /// doesn't expose a per-backend policy mask yet; until it does,
    /// the cffi route has to assume the registered backend supports
    /// the union of every policy any nros-supported RMW honours.
    /// Backends that don't support a policy MUST surface
    /// `NROS_RMW_RET_INCOMPATIBLE_QOS` from `create_publisher` etc.
    /// to keep the no-silent-degradation contract.
    ///
    /// TODO 115.K.2.x: extend `nros_rmw_vtable_t` with a
    /// `supported_qos_policies()` callback so the runtime queries
    /// the backend instead of guessing.
    fn supported_qos_policies(&self) -> nros_rmw::QosPolicyMask {
        use nros_rmw::QosPolicyMask;
        QosPolicyMask::CORE
            | QosPolicyMask::DURABILITY_TRANSIENT_LOCAL
            | QosPolicyMask::AVOID_ROS_NAMESPACE_CONVENTIONS
            | QosPolicyMask::DEADLINE
            | QosPolicyMask::LIFESPAN
            | QosPolicyMask::LIVELINESS_AUTOMATIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_TOPIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_NODE
            | QosPolicyMask::LIVELINESS_LEASE
    }
}

impl Drop for CffiSession {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe {
                (self
                    .vtable
                    .destroy_session
                    .expect("rmw vtable: destroy_session"))(&mut view)
            };
        }
    }
}

// ============================================================================
// CffiPublisher
// ============================================================================

/// Publisher backed by a C vtable.
pub struct CffiPublisher {
    vtable: &'static NrosRmwVtable,
    topic_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    qos: NrosRmwQos,
    can_loan_messages: bool,
    backend_data: *mut c_void,
}

impl CffiPublisher {
    fn make_view(&mut self) -> NrosRmwPublisher {
        NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        }
    }

    /// Topic name. Result is the null-terminated string written at
    /// publisher creation; never re-resolved from the backend.
    pub fn topic_name(&self) -> &str {
        cstr_buf_to_str(&self.topic_name_buf)
    }

    /// Fully-qualified type name (`"std_msgs/msg/Int32"`).
    pub fn type_name(&self) -> &str {
        cstr_buf_to_str(&self.type_name_buf)
    }

    /// QoS used to create this publisher.
    pub fn qos(&self) -> NrosRmwQos {
        self.qos
    }

    /// `true` iff the backend exposes the publish loan primitive
    /// (Phase 99). Mirrors upstream `rmw_publisher_t::can_loan_messages`.
    pub fn can_loan_messages(&self) -> bool {
        self.can_loan_messages
    }
}

/// Phase 124.A — writable slot returned by
/// [`CffiPublisher::try_lend_slot`]. Holds the backend's raw buffer
/// and opaque token until `commit_slot` consumes it or `Drop` fires
/// `pub_discard`.
#[cfg(feature = "lending")]
pub struct CffiSlot<'a> {
    buf: *mut u8,
    cap: usize,
    cursor: usize,
    token: *mut c_void,
    /// `None` after `commit_slot` consumes the slot — Drop skips the
    /// discard call in that case.
    publisher: Option<&'a CffiPublisher>,
    /// Phase 124.A.3 — `true` when this slot came from the runtime's
    /// arena fallback (backend had NULL `pub_loan`). Commit performs
    /// a `publish_raw` of the staged bytes; discard / Drop reclaims
    /// the staging buffer. `false` for native backend loans —
    /// commit / discard go through the vtable slots.
    fallback: bool,
}

#[cfg(feature = "lending")]
impl<'a> CffiSlot<'a> {
    /// Mark the actual bytes written before commit. Defaults to the
    /// full capacity; callers that write a shorter prefix MUST call
    /// `set_len` first.
    pub fn set_len(&mut self, len: usize) {
        debug_assert!(len <= self.cap);
        self.cursor = len.min(self.cap);
    }
}

/// Phase 124.A.3 — staging buffer for the arena-fallback loan path.
/// Allocated on each `try_lend_slot` when the backend's `pub_loan`
/// slot is NULL; commit copies into a `publish_raw` call; Drop /
/// discard reclaims the allocation. `Box::into_raw` of this struct
/// becomes the slot's opaque `token` so commit / discard can find
/// it back.
#[cfg(all(feature = "lending", feature = "alloc"))]
struct ArenaStaging {
    buf: alloc::vec::Vec<u8>,
}

#[cfg(feature = "lending")]
impl<'a> AsMut<[u8]> for CffiSlot<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: `buf` came from `pub_loan` with capacity `cap`. The
        // loan contract guarantees the slot stays valid until commit
        // or discard. The lifetime `'a` borrows the publisher so the
        // returned slice can't outlive the loan.
        unsafe { core::slice::from_raw_parts_mut(self.buf, self.cap) }
    }
}

#[cfg(feature = "lending")]
impl<'a> Drop for CffiSlot<'a> {
    fn drop(&mut self) {
        if self.publisher.is_none() {
            // commit_slot consumed the loan — nothing to release.
            return;
        }
        if self.fallback {
            // Phase 124.A.3 — reclaim the staging allocation.
            #[cfg(feature = "alloc")]
            unsafe {
                let _ = alloc::boxed::Box::from_raw(self.token as *mut ArenaStaging);
            }
            return;
        }
        if let Some(p) = self.publisher
            && let Some(discard) = p.vtable.pub_discard
        {
            // Re-materialise the publisher view so the backend sees
            // the same `NrosRmwPublisher` shape it created the loan
            // against.
            let mut view = NrosRmwPublisher {
                topic_name: p.topic_name_buf.as_ptr().cast(),
                type_name: p.type_name_buf.as_ptr().cast(),
                qos: p.qos,
                can_loan_messages: p.can_loan_messages,
                _reserved: [0u8; 7],
                backend_data: p.backend_data,
            };
            // SAFETY: `token` came from a paired `pub_loan` on this
            // publisher and the publisher is still alive (lifetime
            // `'a` borrows it).
            unsafe { discard(&mut view, self.token) };
        }
    }
}

#[cfg(feature = "lending")]
impl nros_rmw::SlotLending for CffiPublisher {
    type Slot<'a> = CffiSlot<'a>;

    fn try_lend_slot(&self, len: usize) -> Result<Option<CffiSlot<'_>>, TransportError> {
        let Some(loan) = self.vtable.pub_loan else {
            // Phase 124.A.3 — backend doesn't natively lend; allocate
            // a staging buffer and stash it in `token` so commit can
            // memcpy → publish_raw and discard / Drop can reclaim.
            // Requires `alloc` for the dynamic staging; no_std-no_alloc
            // builds return None and let the caller fall back to a
            // non-loan path.
            #[cfg(feature = "alloc")]
            {
                let mut staging = alloc::boxed::Box::new(ArenaStaging {
                    buf: alloc::vec![0u8; len],
                });
                let buf_ptr = staging.buf.as_mut_ptr();
                let token = alloc::boxed::Box::into_raw(staging) as *mut c_void;
                return Ok(Some(CffiSlot {
                    buf: buf_ptr,
                    cap: len,
                    cursor: len,
                    token,
                    publisher: Some(self),
                    fallback: true,
                }));
            }
            #[cfg(not(feature = "alloc"))]
            {
                let _ = len;
                return Ok(None);
            }
        };
        let mut view = NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        let mut out_buf: *mut u8 = core::ptr::null_mut();
        let mut out_cap: usize = 0;
        let mut out_token: *mut c_void = core::ptr::null_mut();
        // SAFETY: vtable contract — slot pointers stay valid until
        // commit / discard.
        let ret = unsafe { loan(&mut view, len, &mut out_buf, &mut out_cap, &mut out_token) };
        if ret == NROS_RMW_RET_WOULD_BLOCK || ret == NROS_RMW_RET_NO_DATA {
            return Ok(None);
        }
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        if out_buf.is_null() || out_cap < len {
            // Defensive: a buggy backend returned OK with a too-small
            // slot. Treat as transient.
            if let Some(discard) = self.vtable.pub_discard {
                unsafe { discard(&mut view, out_token) };
            }
            return Ok(None);
        }
        Ok(Some(CffiSlot {
            buf: out_buf,
            cap: out_cap,
            cursor: len,
            token: out_token,
            publisher: Some(self),
            fallback: false,
        }))
    }

    fn commit_slot(&self, mut slot: CffiSlot<'_>) -> Result<(), TransportError> {
        // Cancel Drop's discard — we're committing, not abandoning.
        let publisher = slot
            .publisher
            .take()
            .ok_or(TransportError::InvalidArgument)?;
        debug_assert!(core::ptr::eq(publisher, self));
        if slot.fallback {
            // Phase 124.A.3 — fallback path: reclaim the staging
            // box, run a single publish_raw of the cursor-truncated
            // contents.
            #[cfg(feature = "alloc")]
            {
                // SAFETY: `slot.token` came from
                // `Box::into_raw(Box<ArenaStaging>)` in try_lend_slot.
                let staging =
                    unsafe { alloc::boxed::Box::from_raw(slot.token as *mut ArenaStaging) };
                let bytes = &staging.buf[..slot.cursor.min(staging.buf.len())];
                return Publisher::publish_raw(self, bytes);
            }
            #[cfg(not(feature = "alloc"))]
            {
                return Err(TransportError::Unsupported);
            }
        }
        let commit = self.vtable.pub_commit.ok_or(TransportError::Unsupported)?;
        let mut view = NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        let len = slot.cursor;
        let token = slot.token;
        // `slot` drops here without firing `pub_discard` because
        // `publisher` is `None`.
        let ret = unsafe { commit(&mut view, token, len) };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }
}

impl Publisher for CffiPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut view = NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        let ret = unsafe {
            (self.vtable.publish_raw.expect("rmw vtable: publish_raw"))(
                &mut view,
                data.as_ptr(),
                data.len(),
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }

    unsafe fn publish_streamed(
        &self,
        size_cb: unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut core::ffi::c_void),
        chunk_cb: unsafe extern "C" fn(
            out_buf: *mut u8,
            cap: usize,
            out_written: *mut usize,
            user_ctx: *mut core::ffi::c_void,
        ),
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        // Phase 124.E.1+2 — vtable forwarder. If the backend exposes
        // `publish_streamed` natively, dispatch in one hop so the
        // callbacks land directly inside the backend's outbound
        // buffer (no staging copy). Otherwise fall back to the
        // `Publisher::publish_streamed` default body, which runs a
        // stack staging buffer + `publish_raw`.
        if let Some(f) = self.vtable.publish_streamed {
            let mut view = NrosRmwPublisher {
                topic_name: self.topic_name_buf.as_ptr().cast(),
                type_name: self.type_name_buf.as_ptr().cast(),
                qos: self.qos,
                can_loan_messages: self.can_loan_messages,
                _reserved: [0u8; 7],
                backend_data: self.backend_data,
            };
            // Generated slot takes nullable callbacks; ours are live fn pointers.
            let ret = unsafe { f(&mut view, Some(size_cb), Some(chunk_cb), user_ctx) };
            if ret != NROS_RMW_RET_OK {
                return Err(error_from_ret(ret));
            }
            return Ok(());
        }
        // Inlined staging-buffer fallback. Mirrors the trait default
        // body so the override doesn't recurse through dynamic
        // dispatch — the default body would resolve back to this
        // function and deadlock.
        const STAGE_CAP: usize = 4096;
        let mut total = 0usize;
        unsafe { size_cb(&mut total as *mut usize, user_ctx) };
        if total > STAGE_CAP {
            return Err(TransportError::BufferTooSmall);
        }
        let mut stage = [0u8; STAGE_CAP];
        let mut written_so_far = 0usize;
        while written_so_far < total {
            let mut chunk_written = 0usize;
            let remaining = total - written_so_far;
            unsafe {
                chunk_cb(
                    stage.as_mut_ptr().add(written_so_far),
                    remaining,
                    &mut chunk_written as *mut usize,
                    user_ctx,
                );
            }
            if chunk_written == 0 {
                return Err(TransportError::BufferTooSmall);
            }
            written_so_far += chunk_written;
        }
        self.publish_raw(&stage[..total])
    }

    fn buffer_error(&self) -> TransportError {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> TransportError {
        TransportError::SerializationError
    }

    fn unsupported_event_error(&self) -> TransportError {
        TransportError::Unsupported
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        let mut view = NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        // Cffi event callback ABI matches nros_rmw::EventCallback (layout
        // notes in `rust_adapter`); the generated slot is nullable, so the
        // live fn pointer is wrapped in `Some`.
        let cb: NrosRmwEventCallback = Some(unsafe {
            core::mem::transmute::<
                nros_rmw::EventCallback,
                unsafe extern "C" fn(NrosRmwEventKind, *const NrosRmwEventPayload, *mut c_void),
            >(cb)
        });
        let ret = unsafe {
            (self
                .vtable
                .register_publisher_event
                .expect("rmw vtable: register_publisher_event"))(
                &mut view,
                event_kind_to_c(kind),
                deadline_ms,
                cb,
                user_ctx,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }

    fn assert_liveliness(&self) -> Result<(), TransportError> {
        // Phase 108.B — manual liveliness assertion. NULL function
        // pointer = backend doesn't support manual liveliness; the
        // runtime caller (Node) gates the call by liveliness_kind so
        // we just delegate.
        let view_ptr = self as *const _ as *mut Self;
        let mut view = unsafe { (*view_ptr).make_view() };
        let ret = unsafe {
            (self
                .vtable
                .assert_publisher_liveliness
                .expect("rmw vtable: assert_publisher_liveliness"))(&mut view)
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }
}

impl Drop for CffiPublisher {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe {
                (self
                    .vtable
                    .destroy_publisher
                    .expect("rmw vtable: destroy_publisher"))(&mut view)
            };
        }
    }
}

// ============================================================================
// CffiSubscription
// ============================================================================

/// Subscription backed by a C vtable.
pub struct CffiSubscription {
    vtable: &'static NrosRmwVtable,
    topic_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    qos: NrosRmwQos,
    can_loan_messages: bool,
    backend_data: *mut c_void,
    /// Phase 231 (RFC-0038) — cached `subscription_supports_in_place` capability,
    /// queried once at creation so `supports_process_in_place(&self)` is cheap.
    supports_in_place: bool,
}

impl CffiSubscription {
    fn make_view(&mut self) -> NrosRmwSubscription {
        NrosRmwSubscription {
            topic_name: self.topic_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        }
    }

    /// Phase 231 (RFC-0038) — drive the `process_raw_in_place` vtable slot,
    /// marshalling the Rust `FnOnce` through the C `ctx`/`cb`. A monomorphized
    /// trampoline takes the closure out of a stack `Option` cell and calls it
    /// with the borrowed slice. The named generic `G` is why the public trait
    /// method (which uses APIT) delegates here.
    fn run_process_in_place<G: FnOnce(&[u8])>(&mut self, f: G) -> Result<bool, TransportError> {
        let Some(slot) = self.vtable.process_raw_in_place else {
            return Err(TransportError::MessageTooLarge);
        };
        unsafe extern "C" fn cb_tramp<G: FnOnce(&[u8])>(
            ctx: *mut c_void,
            ptr: *const u8,
            len: usize,
        ) {
            let cell = unsafe { &mut *(ctx as *mut Option<G>) };
            if let Some(g) = cell.take() {
                g(unsafe { core::slice::from_raw_parts(ptr, len) });
            }
        }
        let mut cell: Option<G> = Some(f);
        let mut view = self.make_view();
        let rc = unsafe {
            slot(
                &mut view,
                &mut cell as *mut Option<G> as *mut c_void,
                Some(cb_tramp::<G>),
            )
        };
        if rc == NROS_RMW_RET_NO_DATA {
            Ok(false)
        } else if rc < 0 {
            Err(error_from_ret(rc))
        } else {
            Ok(rc > 0)
        }
    }

    pub fn topic_name(&self) -> &str {
        cstr_buf_to_str(&self.topic_name_buf)
    }

    pub fn type_name(&self) -> &str {
        cstr_buf_to_str(&self.type_name_buf)
    }

    pub fn qos(&self) -> NrosRmwQos {
        self.qos
    }

    /// `true` iff the backend exposes the receive loan primitive
    /// (Phase 99).
    pub fn can_loan_messages(&self) -> bool {
        self.can_loan_messages
    }
}

/// Phase 124.A — read-only view returned by
/// [`CffiSubscription::try_borrow`]. Holds the backend's raw buffer +
/// opaque token until `Drop` fires `sub_release`.
#[cfg(feature = "lending")]
pub struct CffiView<'a> {
    buf: *const u8,
    len: usize,
    token: *mut c_void,
    subscriber: Option<&'a mut CffiSubscription>,
}

#[cfg(feature = "lending")]
impl<'a> AsRef<[u8]> for CffiView<'a> {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `buf` came from `sub_borrow` with length `len`.
        // The borrow contract guarantees the buffer stays valid until
        // `sub_release` fires (in Drop). Lifetime `'a` borrows the
        // subscriber so the slice can't outlive the borrow.
        unsafe { core::slice::from_raw_parts(self.buf, self.len) }
    }
}

#[cfg(feature = "lending")]
impl<'a> Drop for CffiView<'a> {
    fn drop(&mut self) {
        if let Some(sub) = self.subscriber.take()
            && let Some(release) = sub.vtable.sub_release
        {
            let mut view = sub.make_view();
            // SAFETY: `token` paired with a prior `sub_borrow` on
            // this subscriber and the subscriber is still alive.
            unsafe { release(&mut view, self.token) };
        }
    }
}

#[cfg(feature = "lending")]
impl nros_rmw::SlotBorrowing for CffiSubscription {
    type View<'a> = CffiView<'a>;

    fn try_borrow(&mut self) -> Result<Option<CffiView<'_>>, TransportError> {
        let Some(borrow) = self.vtable.sub_borrow else {
            // Phase 124.A — backend doesn't natively borrow; runtime
            // falls back to `try_recv_raw` into a staging buffer
            // (124.A.3). `None` lets the caller use the slow path.
            return Ok(None);
        };
        let mut view = self.make_view();
        let mut out_buf: *const u8 = core::ptr::null();
        let mut out_len: usize = 0;
        let mut out_token: *mut c_void = core::ptr::null_mut();
        // SAFETY: vtable contract — borrowed pointers stay valid
        // until `sub_release` runs.
        let rc = unsafe { borrow(&mut view, &mut out_buf, &mut out_len, &mut out_token) };
        if rc == 0 {
            // No message ready.
            return Ok(None);
        }
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        if out_buf.is_null() {
            return Ok(None);
        }
        let len = (rc as usize).min(out_len.max(rc as usize));
        Ok(Some(CffiView {
            buf: out_buf,
            len,
            token: out_token,
            subscriber: Some(self),
        }))
    }
}

impl nros_rmw::Subscription for CffiSubscription {
    type Error = TransportError;

    fn supports_process_in_place(&self) -> bool {
        self.supports_in_place
    }

    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error> {
        self.run_process_in_place(f)
    }

    fn has_data(&self) -> bool {
        // has_data takes &mut to match the C signature; cast away const
        // because the predicate is logically read-only — backends must
        // not mutate state from has_data.
        let view_ptr = self as *const _ as *mut Self;
        let mut view = unsafe { (*view_ptr).make_view() };
        let rc = unsafe { (self.vtable.has_data.expect("rmw vtable: has_data"))(&mut view) };
        rc > 0
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        let mut view = self.make_view();
        let rc = unsafe {
            (self.vtable.try_recv_raw.expect("rmw vtable: try_recv_raw"))(
                &mut view,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if rc == NROS_RMW_RET_NO_DATA {
            return Ok(None);
        }
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        if rc == 0 {
            return Ok(None);
        }
        Ok(Some(rc as usize))
    }

    fn try_recv_raw_with_info(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, Option<MessageInfo>)>, TransportError> {
        let key = self.backend_data as usize;
        self.try_recv_raw(buf)
            .map(|opt| opt.map(|len| (len, take_cffi_message_info(key))))
    }

    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    fn try_recv_validated(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, nros_rmw::IntegrityStatus)>, Self::Error> {
        let key = self.backend_data as usize;
        request_cffi_integrity_status(key);
        self.try_recv_raw(buf).map(|opt| {
            opt.map(|len| {
                (
                    len,
                    take_cffi_integrity_status(key).unwrap_or(nros_rmw::IntegrityStatus {
                        gap: 0,
                        duplicate: false,
                        crc_valid: None,
                    }),
                )
            })
        })
    }

    fn try_recv_sequence(
        &mut self,
        buf: &mut [u8],
        per_msg_cap: usize,
        max_msgs: usize,
        out_lens: &mut [usize],
    ) -> Result<usize, TransportError> {
        // Phase 124.D.2 — runtime fallback. If the backend exposes
        // `try_recv_sequence` natively, call it in one hop; otherwise
        // delegate to the trait's default body which loop-drives
        // `try_recv_raw`. Either way the caller sees the same shape:
        // contiguous slot block + per-slot length array + count
        // return.
        if let Some(f) = self.vtable.try_recv_sequence {
            if per_msg_cap == 0 || max_msgs == 0 {
                return Ok(0);
            }
            let limit = max_msgs.min(out_lens.len());
            if buf.len() < limit.saturating_mul(per_msg_cap) {
                return Err(TransportError::BufferTooSmall);
            }
            let mut view = self.make_view();
            let rc = unsafe {
                f(
                    &mut view,
                    buf.as_mut_ptr(),
                    per_msg_cap,
                    limit,
                    out_lens.as_mut_ptr(),
                )
            };
            if rc < 0 {
                return Err(error_from_ret(rc));
            }
            return Ok(rc as usize);
        }
        // Phase 124.D.2 — `try_recv_raw` loop fallback. Inlined
        // here (rather than dispatching back through the trait
        // default body) so the recursion is structurally
        // impossible — `Subscription::try_recv_sequence` on
        // `CffiSubscription` is THIS function, and forwarding to
        // the default body would deadlock the override.
        if per_msg_cap == 0 || max_msgs == 0 {
            return Ok(0);
        }
        let limit = max_msgs.min(out_lens.len());
        if buf.len() < limit.saturating_mul(per_msg_cap) {
            return Err(TransportError::BufferTooSmall);
        }
        let mut count = 0;
        for i in 0..limit {
            let slot = &mut buf[i * per_msg_cap..(i + 1) * per_msg_cap];
            match self.try_recv_raw(slot)? {
                Some(len) => {
                    out_lens[i] = len;
                    count += 1;
                }
                None => break,
            }
        }
        Ok(count)
    }

    fn deserialization_error(&self) -> TransportError {
        TransportError::DeserializationError
    }

    fn unsupported_event_error(&self) -> TransportError {
        TransportError::Unsupported
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        let mut view = self.make_view();
        let cb: NrosRmwEventCallback = Some(unsafe {
            core::mem::transmute::<
                nros_rmw::EventCallback,
                unsafe extern "C" fn(NrosRmwEventKind, *const NrosRmwEventPayload, *mut c_void),
            >(cb)
        });
        let ret = unsafe {
            (self
                .vtable
                .register_subscription_event
                .expect("rmw vtable: register_subscription_event"))(
                &mut view,
                event_kind_to_c(kind),
                deadline_ms,
                cb,
                user_ctx,
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }
}

impl Drop for CffiSubscription {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            clear_cffi_message_info(self.backend_data as usize);
            let mut view = self.make_view();
            unsafe {
                (self
                    .vtable
                    .destroy_subscription
                    .expect("rmw vtable: destroy_subscription"))(&mut view)
            };
        }
    }
}

// ============================================================================
// CffiService
// ============================================================================

/// Service server backed by a C vtable.
pub struct CffiService {
    vtable: &'static NrosRmwVtable,
    service_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    backend_data: *mut c_void,
}

impl CffiService {
    fn make_view(&mut self) -> NrosRmwService {
        NrosRmwService {
            service_name: self.service_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        }
    }

    pub fn service_name(&self) -> &str {
        cstr_buf_to_str(&self.service_name_buf)
    }

    pub fn type_name(&self) -> &str {
        cstr_buf_to_str(&self.type_name_buf)
    }
}

impl ServiceTrait for CffiService {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        let view_ptr = self as *const _ as *mut Self;
        let mut view = unsafe { (*view_ptr).make_view() };
        let rc = unsafe { (self.vtable.has_request.expect("rmw vtable: has_request"))(&mut view) };
        rc > 0
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        let mut seq: i64 = 0;
        let mut view = self.make_view();
        let rc = unsafe {
            (self
                .vtable
                .try_recv_request
                .expect("rmw vtable: try_recv_request"))(
                &mut view,
                buf.as_mut_ptr(),
                buf.len(),
                &mut seq,
            )
        };
        if rc == NROS_RMW_RET_NO_DATA {
            return Ok(None);
        }
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        if rc == 0 {
            return Ok(None);
        }
        let len = rc as usize;
        Ok(Some(ServiceRequest {
            data: &buf[..len],
            sequence_number: seq,
        }))
    }

    fn send_reply(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), TransportError> {
        let mut view = self.make_view();
        let ret = unsafe {
            (self.vtable.send_reply.expect("rmw vtable: send_reply"))(
                &mut view,
                sequence_number,
                data.as_ptr(),
                data.len(),
            )
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }
}

impl Drop for CffiService {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe {
                (self
                    .vtable
                    .destroy_service
                    .expect("rmw vtable: destroy_service"))(&mut view)
            };
        }
    }
}

// ============================================================================
// CffiClient
// ============================================================================

/// Service client backed by a C vtable.
pub struct CffiClient {
    vtable: &'static NrosRmwVtable,
    service_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    backend_data: *mut c_void,
}

impl CffiClient {
    fn make_view(&mut self) -> NrosRmwClient {
        NrosRmwClient {
            service_name: self.service_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        }
    }

    pub fn service_name(&self) -> &str {
        cstr_buf_to_str(&self.service_name_buf)
    }

    pub fn type_name(&self) -> &str {
        cstr_buf_to_str(&self.type_name_buf)
    }
}

impl ClientTrait for CffiClient {
    type Error = TransportError;

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), TransportError> {
        // Phase-301 (issue 0240) — `send_request_raw` +
        // `try_recv_reply_raw` is the ONE request/reply path (the
        // blocking `call_raw` slot is gone from the vtable). Backends
        // that omit the slot get `Unsupported`; the executor surfaces
        // the error instead of silently degrading.
        let Some(f) = self.vtable.send_request_raw else {
            return Err(TransportError::Unsupported);
        };
        let mut view = self.make_view();
        let rc = unsafe { f(&mut view, request.as_ptr(), request.len()) };
        if rc != NROS_RMW_RET_OK {
            return Err(error_from_ret(rc));
        }
        Ok(())
    }

    fn try_recv_reply_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        // Non-blocking poll only. NULL slot = backend doesn't implement
        // the service-client path; surface Unsupported.
        let Some(f) = self.vtable.try_recv_reply_raw else {
            return Err(TransportError::Unsupported);
        };
        let mut view = self.make_view();
        let rc = unsafe { f(&mut view, reply_buf.as_mut_ptr(), reply_buf.len()) };
        if rc == NROS_RMW_RET_NO_DATA {
            return Ok(None);
        }
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        Ok(Some(rc as usize))
    }

    fn server_available(&self) -> Result<bool, TransportError> {
        let Some(f) = self.vtable.service_server_available else {
            return Err(TransportError::Unsupported);
        };
        // SAFETY: `f` accepts a `*mut NrosRmwClient`. We
        // construct a transient view from this client's fields the
        // same way `make_view` does, but on `&self` (no mutation
        // required for a graph probe). The borrowed pointers all
        // alias into `&self`, so the lifetime is bounded by the
        // call.
        let mut view = NrosRmwClient {
            service_name: self.service_name_buf.as_ptr().cast(),
            type_name: self.type_name_buf.as_ptr().cast(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        };
        let rc = unsafe { f(&mut view) };
        match rc {
            0 => Ok(false),
            1 => Ok(true),
            n if n < 0 => Err(error_from_ret(n)),
            // Any positive value other than 1 is non-spec; treat as
            // "server available" — backends signalling availability
            // counts ≥ 1 still mean "ready".
            _ => Ok(true),
        }
    }
}

impl Drop for CffiClient {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe {
                (self
                    .vtable
                    .destroy_client
                    .expect("rmw vtable: destroy_client"))(&mut view)
            };
        }
    }
}

// ============================================================================
// Factory
// ============================================================================

/// RMW factory for the C function table backend.
#[derive(Default)]
pub struct CffiRmw;

impl nros_rmw::Rmw for CffiRmw {
    type Session = CffiSession;
    type Error = TransportError;

    fn open(self, config: &nros_rmw::RmwConfig) -> Result<CffiSession, TransportError> {
        let mode = match config.mode {
            nros_rmw::SessionMode::Client => 0u8,
            nros_rmw::SessionMode::Peer => 1u8,
        };
        CffiSession::open(config.locator, mode, config.domain_id, config.node_name)
    }
}

impl CffiRmw {
    /// Phase 104.C.1 — open a session against a named backend.
    /// `rmw_name` selects an entry from the registry populated by
    /// `nros_rmw_cffi_register_named` (Phase 104.B.2).
    pub fn open_with_rmw(
        rmw_name: &str,
        config: &nros_rmw::RmwConfig,
    ) -> Result<CffiSession, TransportError> {
        let mode = match config.mode {
            nros_rmw::SessionMode::Client => 0u8,
            nros_rmw::SessionMode::Peer => 1u8,
        };
        CffiSession::open_named(
            rmw_name,
            config.locator,
            mode,
            config.domain_id,
            config.node_name,
        )
    }
}

// ============================================================================
// Phase 102.5 — typed-struct roundtrip test
// ============================================================================
//
// Verifies the visible-struct contract end-to-end:
// 1. Runtime fills `topic_name` / `type_name` / `qos` before
//    `create_publisher`.
// 2. Backend's `create_publisher` writes `backend_data` and
//    `can_loan_messages` into the same struct.
// 3. Rust accessors (`CffiPublisher::topic_name()`, `qos()`,
//    `can_loan_messages()`) read back the values without any
//    vtable callback.

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;
    use nros_rmw::{Rmw, RmwConfig, Session, SessionMode, TopicInfo};

    // Stub backend state. Statically allocated; the vtable's
    // `backend_data` round-trips a `&'static mut StubBackend`.
    static mut STUB_OPEN_CALLED: bool = false;
    static mut STUB_CREATE_PUB_CALLED: bool = false;
    static mut STUB_PUBLISH_CALLED: bool = false;
    static mut STUB_LAST_TOPIC_NAME: [u8; 64] = [0u8; 64];
    static mut STUB_LAST_TYPE_NAME: [u8; 64] = [0u8; 64];
    static mut STUB_LAST_QOS: NrosRmwQos = NrosRmwQos {
        reliability: 0,
        durability: 0,
        history: 0,
        liveliness_kind: 0,
        depth: 0,
        _reserved0: 0,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: 0,
        _reserved1: [0; 3],
    };

    /// Read a null-terminated `*const u8` into the supplied byte
    /// buffer. Used by the stub backend to capture the topic / type
    /// names that the runtime hands it.
    unsafe fn copy_cstr(src: *const core::ffi::c_char, dst: &mut [u8]) {
        let src = src.cast::<u8>();
        let mut i = 0;
        while i < dst.len() {
            let b = unsafe { *src.add(i) };
            dst[i] = b;
            if b == 0 {
                break;
            }
            i += 1;
        }
    }

    unsafe extern "C" fn stub_create_session(
        _locator: *const core::ffi::c_char,
        _mode: u8,
        _domain_id: u32,
        _node_name: *const core::ffi::c_char,
        out: *mut NrosRmwSession,
    ) -> NrosRmwRet {
        unsafe {
            STUB_OPEN_CALLED = true;
            (*out).backend_data = 0xDEAD_BEEFusize as *mut c_void;
        }
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_destroy_session(_session: *mut NrosRmwSession) -> NrosRmwRet {
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_drive_io(
        _session: *mut NrosRmwSession,
        _timeout_ms: i32,
    ) -> NrosRmwRet {
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_create_publisher(
        _session: *mut NrosRmwSession,
        _topic_name: *const core::ffi::c_char,
        _type_name: *const core::ffi::c_char,
        _type_hash: *const core::ffi::c_char,
        _domain_id: u32,
        qos: *const NrosRmwQos,
        _options: *const nros_rmw_publisher_options_t,
        out: *mut NrosRmwPublisher,
    ) -> NrosRmwRet {
        // Capture the typed-struct fields the runtime supplied.
        unsafe {
            STUB_CREATE_PUB_CALLED = true;
            copy_cstr((*out).topic_name, &mut STUB_LAST_TOPIC_NAME);
            copy_cstr((*out).type_name, &mut STUB_LAST_TYPE_NAME);
            STUB_LAST_QOS = *qos;
            (*out).backend_data = 0xCAFEusize as *mut c_void;
            (*out).can_loan_messages = true;
        }
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_destroy_publisher(_publisher: *mut NrosRmwPublisher) {}

    unsafe extern "C" fn stub_publish_raw(
        publisher: *mut NrosRmwPublisher,
        _data: *const u8,
        _len: usize,
    ) -> NrosRmwRet {
        // Verify the runtime is still passing the same backend_data
        // and topic_name on every call.
        unsafe {
            STUB_PUBLISH_CALLED = true;
            assert_eq!((*publisher).backend_data as usize, 0xCAFE);
            let mut buf = [0u8; 64];
            copy_cstr((*publisher).topic_name, &mut buf);
            assert_eq!(&buf[..], &STUB_LAST_TOPIC_NAME);
        }
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_create_subscription(
        _: *mut NrosRmwSession,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: u32,
        _: *const NrosRmwQos,
        _: *const nros_rmw_subscription_options_t,
        out: *mut NrosRmwSubscription,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = core::ptr::dangling_mut::<c_void>();
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) {}
    unsafe extern "C" fn stub_try_recv_raw(
        _: *mut NrosRmwSubscription,
        _: *mut u8,
        _: usize,
    ) -> i32 {
        0
    }
    unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscription) -> i32 {
        0
    }

    unsafe extern "C" fn stub_create_service(
        _: *mut NrosRmwSession,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: u32,
        _: *const NrosRmwQos,
        out: *mut NrosRmwService,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = core::ptr::dangling_mut::<c_void>();
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_service(_: *mut NrosRmwService) {}
    unsafe extern "C" fn stub_try_recv_request(
        _: *mut NrosRmwService,
        _: *mut u8,
        _: usize,
        _: *mut i64,
    ) -> i32 {
        NROS_RMW_RET_NO_DATA
    }
    unsafe extern "C" fn stub_has_request(_: *mut NrosRmwService) -> i32 {
        0
    }
    unsafe extern "C" fn stub_send_reply(
        _: *mut NrosRmwService,
        _: i64,
        _: *const u8,
        _: usize,
    ) -> NrosRmwRet {
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_create_client(
        _: *mut NrosRmwSession,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: *const core::ffi::c_char,
        _: u32,
        _: *const NrosRmwQos,
        out: *mut NrosRmwClient,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = core::ptr::dangling_mut::<c_void>();
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) {}
    unsafe extern "C" fn stub_register_subscription_event(
        _: *mut NrosRmwSubscription,
        _: NrosRmwEventKind,
        _: u32,
        _: NrosRmwEventCallback,
        _: *mut c_void,
    ) -> NrosRmwRet {
        NROS_RMW_RET_UNSUPPORTED
    }
    unsafe extern "C" fn stub_register_publisher_event(
        _: *mut NrosRmwPublisher,
        _: NrosRmwEventKind,
        _: u32,
        _: NrosRmwEventCallback,
        _: *mut c_void,
    ) -> NrosRmwRet {
        NROS_RMW_RET_UNSUPPORTED
    }
    unsafe extern "C" fn stub_assert_publisher_liveliness(_: *mut NrosRmwPublisher) -> NrosRmwRet {
        NROS_RMW_RET_UNSUPPORTED
    }

    static STUB_VTABLE: NrosRmwVtable = NrosRmwVtable {
        create_session: Some(stub_create_session),
        destroy_session: Some(stub_destroy_session),
        drive_io: Some(stub_drive_io),
        create_publisher: Some(stub_create_publisher),
        destroy_publisher: Some(stub_destroy_publisher),
        publish_raw: Some(stub_publish_raw),
        create_subscription: Some(stub_create_subscription),
        destroy_subscription: Some(stub_destroy_subscription),
        try_recv_raw: Some(stub_try_recv_raw),
        has_data: Some(stub_has_data),
        create_service: Some(stub_create_service),
        destroy_service: Some(stub_destroy_service),
        try_recv_request: Some(stub_try_recv_request),
        has_request: Some(stub_has_request),
        send_reply: Some(stub_send_reply),
        create_client: Some(stub_create_client),
        destroy_client: Some(stub_destroy_client),
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscription_event: Some(stub_register_subscription_event),
        register_publisher_event: Some(stub_register_publisher_event),
        assert_publisher_liveliness: Some(stub_assert_publisher_liveliness),
        next_deadline_ms: None,
        set_wake_callback: None,
        pub_loan: None,
        pub_commit: None,
        pub_discard: None,
        sub_borrow: None,
        sub_release: None,
        service_server_available: None,
        try_recv_sequence: None,
        publish_streamed: None,
        ping_session: None,
        subscription_supports_in_place: None,
        process_raw_in_place: None,
    };

    // Phase-301 (issue 0241) — boundary semantics of the QoS lowering.

    #[test]
    fn qos_depth_at_u16_max_lowers() {
        let qos = nros_rmw::QosSettings::default().keep_last(u16::MAX as u32);
        let lowered = NrosRmwQos::try_from(qos).expect("depth 65535 must lower");
        assert_eq!(lowered.depth, u16::MAX);
    }

    #[test]
    fn qos_depth_past_u16_max_is_create_time_error() {
        let qos = nros_rmw::QosSettings::default().keep_last(u16::MAX as u32 + 1);
        assert_eq!(
            NrosRmwQos::try_from(qos),
            Err(TransportError::InvalidArgument)
        );
    }

    #[test]
    fn qos_infinite_sentinel_passes_through_and_reads_as_unset() {
        use nros_rmw::{DURATION_INFINITE_MS, QosPolicyMask};
        let mut qos = nros_rmw::QosSettings::default();
        qos.deadline_ms = DURATION_INFINITE_MS;
        qos.lifespan_ms = DURATION_INFINITE_MS;
        qos.liveliness_lease_ms = DURATION_INFINITE_MS;
        // Sentinel behaves like 0 at the check sites: no extra policy demanded.
        let required = qos.required_policies();
        assert!(!required.contains(QosPolicyMask::DEADLINE));
        assert!(!required.contains(QosPolicyMask::LIFESPAN));
        assert!(!required.contains(QosPolicyMask::LIVELINESS_LEASE));
        // And lowers unchanged — the C side sees the explicit spelling.
        let lowered = NrosRmwQos::try_from(qos).expect("sentinel must lower");
        assert_eq!(lowered.deadline_ms, DURATION_INFINITE_MS);
        assert_eq!(lowered.lifespan_ms, DURATION_INFINITE_MS);
        assert_eq!(lowered.liveliness_lease_ms, DURATION_INFINITE_MS);
    }

    #[test]
    fn duration_lowering_boundaries() {
        use core::time::Duration;
        use nros_rmw::{DURATION_INFINITE_MS, duration_to_qos_ms};
        // 0 keeps its unset/no-check meaning.
        assert_eq!(duration_to_qos_ms(Duration::ZERO), Ok(0));
        // Sub-ms CEILs to 1 ms — never floors to "no deadline".
        assert_eq!(duration_to_qos_ms(Duration::from_nanos(1)), Ok(1));
        assert_eq!(duration_to_qos_ms(Duration::from_micros(999)), Ok(1));
        assert_eq!(duration_to_qos_ms(Duration::from_millis(1)), Ok(1));
        assert_eq!(duration_to_qos_ms(Duration::from_micros(1001)), Ok(2));
        // Largest representable finite value.
        assert_eq!(
            duration_to_qos_ms(Duration::from_millis(DURATION_INFINITE_MS as u64 - 1)),
            Ok(DURATION_INFINITE_MS - 1)
        );
        // At / past the sentinel: create-time error, never a clamp (infinite
        // is spelled via the sentinel or 0, not a huge finite duration).
        assert_eq!(
            duration_to_qos_ms(Duration::from_millis(DURATION_INFINITE_MS as u64)),
            Err(TransportError::InvalidArgument)
        );
        assert_eq!(
            duration_to_qos_ms(Duration::from_secs(u64::MAX / 1_000)),
            Err(TransportError::InvalidArgument)
        );
    }

    #[test]
    fn service_server_no_data_maps_to_none() {
        use nros_rmw::ServiceTrait as _;

        let mut server = CffiService {
            vtable: &STUB_VTABLE,
            service_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::dangling_mut::<c_void>(),
        };
        let mut buf = [0u8; 16];

        assert!(server.try_recv_request(&mut buf).unwrap().is_none());
    }

    #[test]
    fn typed_struct_roundtrip() {
        // Register the stub vtable under its canonical name.
        let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), &STUB_VTABLE) };
        assert_eq!(ret, NROS_RMW_RET_OK);

        // Open a session.
        let cfg = RmwConfig {
            mode: SessionMode::Client,
            locator: "tcp/127.0.0.1:7447",
            domain_id: 0,
            node_name: "test_node",
            namespace: "",
            properties: &[],
        };
        let mut session = Rmw::open(CffiRmw, &cfg).expect("session open");
        assert!(unsafe { STUB_OPEN_CALLED });
        assert_eq!(session.node_name(), "test_node");

        // Create a publisher; verify backend received the typed
        // struct with topic_name + qos populated.
        let topic = TopicInfo::new("/chatter", "std_msgs/msg/Int32", "RIHS01_abc");
        let qos = nros_rmw::QosSettings::default();
        let publisher = session
            .create_publisher(&topic, qos)
            .expect("publisher create");
        assert!(unsafe { STUB_CREATE_PUB_CALLED });
        let topic_buf = unsafe { &STUB_LAST_TOPIC_NAME };
        assert_eq!(
            core::str::from_utf8(topic_buf)
                .unwrap_or("")
                .trim_end_matches('\0'),
            "/chatter"
        );
        let type_buf = unsafe { &STUB_LAST_TYPE_NAME };
        assert_eq!(
            core::str::from_utf8(type_buf)
                .unwrap_or("")
                .trim_end_matches('\0'),
            "std_msgs/msg/Int32"
        );

        // Rust accessors read back the typed-struct fields.
        assert_eq!(publisher.topic_name(), "/chatter");
        assert_eq!(publisher.type_name(), "std_msgs/msg/Int32");
        assert!(publisher.can_loan_messages());

        // Publish — verify backend_data round-trips correctly via
        // the typed view.
        use nros_rmw::Publisher as _;
        publisher.publish_raw(&[1u8, 2, 3]).expect("publish");
        assert!(unsafe { STUB_PUBLISH_CALLED });
    }
}
