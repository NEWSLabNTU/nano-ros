//! C function table adapter for nros RMW backends.
//!
//! This crate provides a vtable-based bridge so that backends written in C,
//! C++, Zig, Ada, or any language with a C-compatible ABI can implement the
//! nros `Session` / `Publisher` / `Subscriber` / service traits without
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

use core::{ffi::c_void, sync::atomic::Ordering};

use portable_atomic::AtomicPtr;

use nros_rmw::{
    Publisher, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings,
    ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session, TopicInfo,
    TransportError,
};

// ============================================================================
// Phase 102.1 — `nros_rmw_ret_t` named return codes
// ============================================================================
//
// Mirrors the macro constants in `<nros/rmw_ret.h>`. The C side uses
// `#define` so future additions don't widen the type; the Rust side
// uses `pub const` so the same names are usable by Rust code that
// crosses the C-vtable boundary.

/// Signed 32-bit status code mirroring the C `nros_rmw_ret_t` typedef.
/// Zero on success; negative on error.
pub type NrosRmwRet = i32;

/// Operation completed successfully.
pub const NROS_RMW_RET_OK: NrosRmwRet = 0;
/// Generic failure not covered by a more specific code.
pub const NROS_RMW_RET_ERROR: NrosRmwRet = -1;
/// Operation deadline elapsed before completion.
pub const NROS_RMW_RET_TIMEOUT: NrosRmwRet = -2;
/// Memory allocation failed.
pub const NROS_RMW_RET_BAD_ALLOC: NrosRmwRet = -3;
/// Caller supplied a NULL pointer or an out-of-range value.
pub const NROS_RMW_RET_INVALID_ARGUMENT: NrosRmwRet = -4;
/// The backend does not implement this operation.
pub const NROS_RMW_RET_UNSUPPORTED: NrosRmwRet = -5;
/// QoS profiles incompatible in a way the backend cannot reconcile.
pub const NROS_RMW_RET_INCOMPATIBLE_QOS: NrosRmwRet = -6;
/// Topic, service, or action name failed validation.
pub const NROS_RMW_RET_TOPIC_NAME_INVALID: NrosRmwRet = -7;
/// Request referenced a node that does not exist in this session.
pub const NROS_RMW_RET_NODE_NAME_NON_EXISTENT: NrosRmwRet = -8;
/// Backend does not support loaned messages on this entity, or slot in use.
pub const NROS_RMW_RET_LOAN_NOT_SUPPORTED: NrosRmwRet = -9;
/// No data on a non-blocking receive (distinct from `TIMEOUT`).
pub const NROS_RMW_RET_NO_DATA: NrosRmwRet = -10;
/// Resource momentarily unavailable; caller should retry.
pub const NROS_RMW_RET_WOULD_BLOCK: NrosRmwRet = -11;
/// Caller buffer smaller than the data the backend wants to deliver.
pub const NROS_RMW_RET_BUFFER_TOO_SMALL: NrosRmwRet = -12;
/// Incoming message exceeded the backend's static capacity.
pub const NROS_RMW_RET_MESSAGE_TOO_LARGE: NrosRmwRet = -13;

// Phase 115.G.4 — anchor every C-stub-transport symbol so they
// survive `--gc-sections` when integration tests link against
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
pub fn _phase_115_g4_anchor() -> [*const core::ffi::c_void; 6] {
    [
        nros_c_stub_make_ops as *const _,
        nros_c_stub_reset_counters as *const _,
        nros_c_stub_get_open_calls as *const _,
        nros_c_stub_get_close_calls as *const _,
        nros_c_stub_get_write_calls as *const _,
        nros_c_stub_get_read_calls as *const _,
    ]
}
/// Phase 115.A.2 — caller's vtable struct has an `abi_version` the
/// runtime doesn't know. Returned by entry points that take a
/// versioned vtable struct (`nros_set_custom_transport`,
/// `nros_cpp_set_custom_transport`, …) when
/// `vtable.abi_version != NROS_RMW_*_ABI_VERSION_VN`.
pub const NROS_RMW_RET_INCOMPATIBLE_ABI: NrosRmwRet = -14;

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
        _ => TransportError::Backend("unknown rmw_ret_t"),
    }
}

// ============================================================================
// Phase 102.3 — typed entity structs (mirrors `<nros/rmw_entity.h>`)
// ============================================================================
//
// These structs are layout-compatible with the typed entity structs
// in the C header. Same shape as upstream `rmw.h`'s `rmw_publisher_t`
// / `rmw_subscription_t` family: visible metadata + a `void * data`
// tail (named `backend_data` here).

/// Liveliness kind values for `NrosRmwQos::liveliness_kind`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NrosRmwLivelinessKind {
    None = 0,
    Automatic = 1,
    ManualByTopic = 2,
    ManualByNode = 3,
}

/// Full DDS-shaped QoS profile. Mirrors `nros_rmw_qos_t` from
/// `<nros/rmw_entity.h>`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NrosRmwQos {
    /// Reliability policy: `0` = best-effort, `1` = reliable.
    pub reliability: u8,
    /// Durability policy: `0` = volatile, `1` = transient-local.
    pub durability: u8,
    /// History policy: `0` = keep-last, `1` = keep-all.
    pub history: u8,
    /// Liveliness kind. See [`NrosRmwLivelinessKind`].
    pub liveliness_kind: u8,
    /// History depth (0–65 535).
    pub depth: u16,
    /// Reserved; must be zero.
    pub _reserved0: u16,

    /// Subscriber max-inter-arrival / publisher offered-rate, ms.
    /// `0` = infinite (no deadline).
    pub deadline_ms: u32,
    /// Sample expiry, ms. `0` = infinite.
    pub lifespan_ms: u32,
    /// Liveliness lease, ms. `0` = infinite.
    pub liveliness_lease_ms: u32,
    /// If non-zero, topic name encoding skips the ROS `/rt/` prefix.
    /// `u8` instead of `bool` for ABI parity with C — `sizeof(_Bool)`
    /// is impl-defined per C99.
    pub avoid_ros_namespace_conventions: u8,
    /// Reserved; must be zero.
    pub _reserved1: [u8; 3],
}

/// Standard `rmw_qos_profile_default`-equivalent.
pub const NROS_RMW_QOS_PROFILE_DEFAULT: NrosRmwQos = NrosRmwQos {
    reliability: 1, // RELIABLE
    durability: 0,  // VOLATILE
    history: 0,     // KEEP_LAST
    liveliness_kind: NrosRmwLivelinessKind::Automatic as u8,
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
    liveliness_kind: NrosRmwLivelinessKind::Automatic as u8,
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

/// Per-process RMW session. Mirrors `nros_rmw_session_t`.
#[repr(C)]
pub struct NrosRmwSession {
    /// Borrowed; outlives the session.
    pub node_name: *const u8,
    /// Borrowed; outlives the session.
    pub namespace_: *const u8,
    /// Reserved for future fields (Phase 104 vtable pointer slot);
    /// must be zero.
    pub _reserved: [u8; 8],
    /// Opaque backend state. NULL when uninitialised.
    pub backend_data: *mut c_void,
}

/// Publisher entity. Mirrors `nros_rmw_publisher_t`.
///
/// `can_loan_messages` matches upstream `rmw_publisher_t`'s field of
/// the same name: `true` means the backend exposes the
/// `loan_publish` / `commit_publish` primitive (Phase 99).
#[repr(C)]
pub struct NrosRmwPublisher {
    /// Borrowed; outlives the publisher.
    pub topic_name: *const u8,
    /// Borrowed; outlives the publisher.
    pub type_name: *const u8,
    pub qos: NrosRmwQos,
    /// Backend exposes loan_publish / commit_publish (Phase 99).
    pub can_loan_messages: bool,
    /// Reserved for future fields; must be zero.
    pub _reserved: [u8; 7],
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Subscriber entity. Mirrors `nros_rmw_subscriber_t`.
#[repr(C)]
pub struct NrosRmwSubscriber {
    /// Borrowed; outlives the subscriber.
    pub topic_name: *const u8,
    /// Borrowed; outlives the subscriber.
    pub type_name: *const u8,
    pub qos: NrosRmwQos,
    /// Backend exposes loan_recv / release_recv (Phase 99).
    pub can_loan_messages: bool,
    /// Reserved for future fields; must be zero.
    pub _reserved: [u8; 7],
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Service-server entity. Mirrors `nros_rmw_service_server_t`.
#[repr(C)]
pub struct NrosRmwServiceServer {
    /// Borrowed; outlives the server.
    pub service_name: *const u8,
    /// Borrowed; outlives the server.
    pub type_name: *const u8,
    /// Reserved for future fields; must be zero.
    pub _reserved: [u8; 8],
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Service-client entity. Mirrors `nros_rmw_service_client_t`.
#[repr(C)]
pub struct NrosRmwServiceClient {
    /// Borrowed; outlives the client.
    pub service_name: *const u8,
    /// Borrowed; outlives the client.
    pub type_name: *const u8,
    /// Reserved for future fields; must be zero.
    pub _reserved: [u8; 8],
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

impl From<QosSettings> for NrosRmwQos {
    fn from(qos: QosSettings) -> Self {
        Self {
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
            // QosSettings::depth is u32; clamp to u16 max. Embedded
            // ROS queue depths are typically 1–100; oversize values
            // are saturated at 65 535 rather than wrapped.
            depth: qos.depth.min(u16::MAX as u32) as u16,
            _reserved0: 0,
            deadline_ms: qos.deadline_ms,
            lifespan_ms: qos.lifespan_ms,
            liveliness_lease_ms: qos.liveliness_lease_ms,
            avoid_ros_namespace_conventions: qos.avoid_ros_namespace_conventions as u8,
            _reserved1: [0; 3],
        }
    }
}

// ============================================================================
// Vtable type (mirrors C header)
// ============================================================================

/// C function table for an RMW backend.
///
/// Mirrors `nros_rmw_vtable_t` from `<nros/rmw_vtable.h>`. Phase 102.4
/// signatures: every entity entry point takes a typed-struct pointer
/// instead of `void *`; every status-only return is `nros_rmw_ret_t`
/// (typedef of `i32`); byte-count returns stay `i32` (positive bytes,
/// negative `nros_rmw_ret_t`).
#[repr(C)]
pub struct NrosRmwVtable {
    // ---- Session lifecycle ----
    pub open: unsafe extern "C" fn(
        locator: *const u8,
        mode: u8,
        domain_id: u32,
        node_name: *const u8,
        out: *mut NrosRmwSession,
    ) -> NrosRmwRet,
    pub close: unsafe extern "C" fn(session: *mut NrosRmwSession) -> NrosRmwRet,
    pub drive_io: unsafe extern "C" fn(session: *mut NrosRmwSession, timeout_ms: i32) -> NrosRmwRet,

    // ---- Publisher ----
    pub create_publisher: unsafe extern "C" fn(
        session: *mut NrosRmwSession,
        topic_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        qos: *const NrosRmwQos,
        out: *mut NrosRmwPublisher,
    ) -> NrosRmwRet,
    pub destroy_publisher: unsafe extern "C" fn(publisher: *mut NrosRmwPublisher),
    pub publish_raw: unsafe extern "C" fn(
        publisher: *mut NrosRmwPublisher,
        data: *const u8,
        len: usize,
    ) -> NrosRmwRet,

    // ---- Subscriber ----
    pub create_subscriber: unsafe extern "C" fn(
        session: *mut NrosRmwSession,
        topic_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        qos: *const NrosRmwQos,
        out: *mut NrosRmwSubscriber,
    ) -> NrosRmwRet,
    pub destroy_subscriber: unsafe extern "C" fn(subscriber: *mut NrosRmwSubscriber),
    pub try_recv_raw: unsafe extern "C" fn(
        subscriber: *mut NrosRmwSubscriber,
        buf: *mut u8,
        buf_len: usize,
    ) -> i32,
    pub has_data: unsafe extern "C" fn(subscriber: *mut NrosRmwSubscriber) -> i32,

    // ---- Service Server ----
    pub create_service_server: unsafe extern "C" fn(
        session: *mut NrosRmwSession,
        service_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        out: *mut NrosRmwServiceServer,
    ) -> NrosRmwRet,
    pub destroy_service_server: unsafe extern "C" fn(server: *mut NrosRmwServiceServer),
    pub try_recv_request: unsafe extern "C" fn(
        server: *mut NrosRmwServiceServer,
        buf: *mut u8,
        buf_len: usize,
        seq_out: *mut i64,
    ) -> i32,
    pub has_request: unsafe extern "C" fn(server: *mut NrosRmwServiceServer) -> i32,
    pub send_reply: unsafe extern "C" fn(
        server: *mut NrosRmwServiceServer,
        seq: i64,
        data: *const u8,
        len: usize,
    ) -> NrosRmwRet,

    // ---- Service Client ----
    pub create_service_client: unsafe extern "C" fn(
        session: *mut NrosRmwSession,
        service_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        out: *mut NrosRmwServiceClient,
    ) -> NrosRmwRet,
    pub destroy_service_client: unsafe extern "C" fn(client: *mut NrosRmwServiceClient),
    pub call_raw: unsafe extern "C" fn(
        client: *mut NrosRmwServiceClient,
        request: *const u8,
        req_len: usize,
        reply_buf: *mut u8,
        reply_buf_len: usize,
    ) -> i32,

    // ---- Phase 108 — status events (optional) ----
    pub register_subscriber_event: unsafe extern "C" fn(
        subscriber: *mut NrosRmwSubscriber,
        kind: NrosRmwEventKind,
        deadline_ms: u32,
        cb: NrosRmwEventCallback,
        user_context: *mut c_void,
    ) -> NrosRmwRet,

    pub register_publisher_event: unsafe extern "C" fn(
        publisher: *mut NrosRmwPublisher,
        kind: NrosRmwEventKind,
        deadline_ms: u32,
        cb: NrosRmwEventCallback,
        user_context: *mut c_void,
    ) -> NrosRmwRet,

    // ---- Phase 108.B — manual liveliness assertion (optional) ----
    pub assert_publisher_liveliness:
        unsafe extern "C" fn(publisher: *mut NrosRmwPublisher) -> NrosRmwRet,

    // ---- Phase 110.0 — backend's next internal-event deadline ----
    /// Returns next deadline in ms (≥ 0) or a negative value for
    /// "no deadline". NULL function pointer = treat as no deadline.
    pub next_deadline_ms: Option<unsafe extern "C" fn(session: *const NrosRmwSession) -> i32>,
}

// ============================================================================
// Phase 108 — status-event types (mirror `<nros/rmw_event.h>`)
// ============================================================================

/// Tier-1 event kinds. Stable u8 values matching
/// `nros_rmw_event_kind_t` in the C header.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NrosRmwEventKind {
    LivelinessChanged = 0,
    RequestedDeadlineMissed = 1,
    MessageLost = 2,
    LivelinessLost = 3,
    OfferedDeadlineMissed = 4,
}

impl From<nros_rmw::EventKind> for NrosRmwEventKind {
    fn from(k: nros_rmw::EventKind) -> Self {
        use nros_rmw::EventKind as K;
        match k {
            K::LivelinessChanged => NrosRmwEventKind::LivelinessChanged,
            K::RequestedDeadlineMissed => NrosRmwEventKind::RequestedDeadlineMissed,
            K::MessageLost => NrosRmwEventKind::MessageLost,
            K::LivelinessLost => NrosRmwEventKind::LivelinessLost,
            K::OfferedDeadlineMissed => NrosRmwEventKind::OfferedDeadlineMissed,
            _ => NrosRmwEventKind::MessageLost, // unreachable for now (#[non_exhaustive])
        }
    }
}

/// Liveliness payload mirror.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NrosRmwLivelinessChangedStatus {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

/// Count payload mirror.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NrosRmwCountStatus {
    pub total_count: u32,
    pub total_count_change: u32,
}

/// Borrow-shaped payload union mirror. C-side ABI — runtime-checked
/// kind tag selects which member is valid.
#[repr(C)]
pub union NrosRmwEventPayload {
    pub liveliness_changed: NrosRmwLivelinessChangedStatus,
    pub count: NrosRmwCountStatus,
}

/// C callback signature. Matches `nros_rmw_event_callback_t`.
pub type NrosRmwEventCallback = unsafe extern "C" fn(
    kind: NrosRmwEventKind,
    payload: *const NrosRmwEventPayload,
    user_context: *mut c_void,
);

// ============================================================================
// Registration
// ============================================================================

static VTABLE: AtomicPtr<NrosRmwVtable> = AtomicPtr::new(core::ptr::null_mut());

/// Register a custom RMW backend vtable.
///
/// # Safety
///
/// The vtable pointer must remain valid for the lifetime of the program.
/// All function pointers in the vtable must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_register(vtable: *const NrosRmwVtable) -> NrosRmwRet {
    VTABLE.store(vtable as *mut NrosRmwVtable, Ordering::Release);
    NROS_RMW_RET_OK
}

fn get_vtable() -> Result<&'static NrosRmwVtable, TransportError> {
    let ptr = VTABLE.load(Ordering::Acquire);
    if ptr.is_null() {
        // No vtable registered — caller forgot nros_rmw_cffi_register.
        return Err(TransportError::InvalidArgument);
    }
    // SAFETY: Registration ensures the pointer is valid and 'static.
    Ok(unsafe { &*ptr })
}

// ============================================================================
// Helper: null-terminated string on the stack
// ============================================================================

/// Write a Rust `&str` as a null-terminated byte sequence into a fixed buffer.
/// Returns a pointer to the buffer start.
fn to_c_str<const N: usize>(s: &str, buf: &mut [u8; N]) -> *const u8 {
    let len = s.len().min(N - 1);
    buf[..len].copy_from_slice(&s.as_bytes()[..len]);
    buf[len] = 0;
    buf.as_ptr()
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
    /// Backend-private state, written by `vtable.open`.
    backend_data: *mut c_void,
}

impl CffiSession {
    fn make_view(&mut self) -> NrosRmwSession {
        NrosRmwSession {
            node_name: self.node_name_buf.as_ptr(),
            namespace_: self.namespace_buf.as_ptr(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        }
    }

    /// Node name passed at session-open time.
    pub fn node_name(&self) -> &str {
        cstr_buf_to_str(&self.node_name_buf)
    }

    /// Open a new session via the registered vtable.
    pub fn open(
        locator: &str,
        mode: u8,
        domain_id: u32,
        node_name: &str,
    ) -> Result<Self, TransportError> {
        let vtable = get_vtable()?;
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
            node_name: session.node_name_buf.as_ptr(),
            namespace_: session.namespace_buf.as_ptr(),
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        let ret = unsafe {
            (vtable.open)(
                loc_ptr,
                mode,
                domain_id,
                session.node_name_buf.as_ptr(),
                &mut view,
            )
        };
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
    type SubscriberHandle = CffiSubscriber;
    type ServiceServerHandle = CffiServiceServer;
    type ServiceClientHandle = CffiServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiPublisher, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let qos_struct = NrosRmwQos::from(qos);

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
        let mut session_view = self.make_view();
        let ret = unsafe {
            (self.vtable.create_publisher)(
                &mut session_view,
                topic_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &qos_struct,
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

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiSubscriber, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let qos_struct = NrosRmwQos::from(qos);

        let mut sub_state = CffiSubscriber {
            vtable: self.vtable,
            topic_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            qos: qos_struct,
            can_loan_messages: false,
            backend_data: core::ptr::null_mut(),
        };
        let topic_ptr = to_c_str(topic.name, &mut sub_state.topic_name_buf);
        let type_ptr = to_c_str(topic.type_name, &mut sub_state.type_name_buf);

        let mut view = NrosRmwSubscriber {
            topic_name: topic_ptr,
            type_name: type_ptr,
            qos: qos_struct,
            can_loan_messages: false,
            _reserved: [0u8; 7],
            backend_data: core::ptr::null_mut(),
        };
        let mut session_view = self.make_view();
        let ret = unsafe {
            (self.vtable.create_subscriber)(
                &mut session_view,
                topic_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &qos_struct,
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
        Ok(sub_state)
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<CffiServiceServer, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let mut srv_state = CffiServiceServer {
            vtable: self.vtable,
            service_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::null_mut(),
        };
        let svc_ptr = to_c_str(service.name, &mut srv_state.service_name_buf);
        let type_ptr = to_c_str(service.type_name, &mut srv_state.type_name_buf);

        let mut view = NrosRmwServiceServer {
            service_name: svc_ptr,
            type_name: type_ptr,
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        let mut session_view = self.make_view();
        let ret = unsafe {
            (self.vtable.create_service_server)(
                &mut session_view,
                svc_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
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

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<CffiServiceClient, TransportError> {
        let mut hash_buf = [0u8; HASH_BUF_LEN];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let mut cli_state = CffiServiceClient {
            vtable: self.vtable,
            service_name_buf: [0u8; NAME_BUF_LEN],
            type_name_buf: [0u8; NAME_BUF_LEN],
            backend_data: core::ptr::null_mut(),
            pending_request: [0u8; 4096],
            pending_len: 0,
        };
        let svc_ptr = to_c_str(service.name, &mut cli_state.service_name_buf);
        let type_ptr = to_c_str(service.type_name, &mut cli_state.type_name_buf);

        let mut view = NrosRmwServiceClient {
            service_name: svc_ptr,
            type_name: type_ptr,
            _reserved: [0u8; 8],
            backend_data: core::ptr::null_mut(),
        };
        let mut session_view = self.make_view();
        let ret = unsafe {
            (self.vtable.create_service_client)(
                &mut session_view,
                svc_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
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
        let ret = unsafe { (self.vtable.close)(&mut view) };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        self.backend_data = core::ptr::null_mut();
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        let mut view = self.make_view();
        let ret = unsafe { (self.vtable.drive_io)(&mut view, timeout_ms) };
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
            node_name: self.node_name_buf.as_ptr(),
            namespace_: self.namespace_buf.as_ptr(),
            _reserved: [0u8; 8],
            backend_data: self.backend_data,
        };
        let ret = unsafe { f(&view as *const _) };
        if ret < 0 { None } else { Some(ret as u32) }
    }
}

impl Drop for CffiSession {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe { (self.vtable.close)(&mut view) };
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
            topic_name: self.topic_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
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

impl Publisher for CffiPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut view = NrosRmwPublisher {
            topic_name: self.topic_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        let ret = unsafe { (self.vtable.publish_raw)(&mut view, data.as_ptr(), data.len()) };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
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
            topic_name: self.topic_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
        };
        // Cffi NrosRmwEventCallback ABI matches nros_rmw::EventCallback —
        // both are `unsafe extern "C" fn(EventKind, *const c_void, *mut c_void)`.
        // The C-side enum is bitwise-equivalent to the Rust enum (same #[repr(u8)]).
        let cb: NrosRmwEventCallback =
            unsafe { core::mem::transmute::<nros_rmw::EventCallback, NrosRmwEventCallback>(cb) };
        let ret = unsafe {
            (self.vtable.register_publisher_event)(
                &mut view,
                kind.into(),
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
        let ret = unsafe { (self.vtable.assert_publisher_liveliness)(&mut view) };
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
            unsafe { (self.vtable.destroy_publisher)(&mut view) };
        }
    }
}

// ============================================================================
// CffiSubscriber
// ============================================================================

/// Subscriber backed by a C vtable.
pub struct CffiSubscriber {
    vtable: &'static NrosRmwVtable,
    topic_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    qos: NrosRmwQos,
    can_loan_messages: bool,
    backend_data: *mut c_void,
}

impl CffiSubscriber {
    fn make_view(&mut self) -> NrosRmwSubscriber {
        NrosRmwSubscriber {
            topic_name: self.topic_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
            qos: self.qos,
            can_loan_messages: self.can_loan_messages,
            _reserved: [0u8; 7],
            backend_data: self.backend_data,
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

impl nros_rmw::Subscriber for CffiSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        // has_data takes &mut to match the C signature; cast away const
        // because the predicate is logically read-only — backends must
        // not mutate state from has_data.
        let view_ptr = self as *const _ as *mut Self;
        let mut view = unsafe { (*view_ptr).make_view() };
        let rc = unsafe { (self.vtable.has_data)(&mut view) };
        rc > 0
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        let mut view = self.make_view();
        let rc = unsafe { (self.vtable.try_recv_raw)(&mut view, buf.as_mut_ptr(), buf.len()) };
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        if rc == 0 {
            return Ok(None);
        }
        Ok(Some(rc as usize))
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
        let cb: NrosRmwEventCallback =
            unsafe { core::mem::transmute::<nros_rmw::EventCallback, NrosRmwEventCallback>(cb) };
        let ret = unsafe {
            (self.vtable.register_subscriber_event)(
                &mut view,
                kind.into(),
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

impl Drop for CffiSubscriber {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe { (self.vtable.destroy_subscriber)(&mut view) };
        }
    }
}

// ============================================================================
// CffiServiceServer
// ============================================================================

/// Service server backed by a C vtable.
pub struct CffiServiceServer {
    vtable: &'static NrosRmwVtable,
    service_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    backend_data: *mut c_void,
}

impl CffiServiceServer {
    fn make_view(&mut self) -> NrosRmwServiceServer {
        NrosRmwServiceServer {
            service_name: self.service_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
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

impl ServiceServerTrait for CffiServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        let view_ptr = self as *const _ as *mut Self;
        let mut view = unsafe { (*view_ptr).make_view() };
        let rc = unsafe { (self.vtable.has_request)(&mut view) };
        rc > 0
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        let mut seq: i64 = 0;
        let mut view = self.make_view();
        let rc = unsafe {
            (self.vtable.try_recv_request)(&mut view, buf.as_mut_ptr(), buf.len(), &mut seq)
        };
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
            (self.vtable.send_reply)(&mut view, sequence_number, data.as_ptr(), data.len())
        };
        if ret != NROS_RMW_RET_OK {
            return Err(error_from_ret(ret));
        }
        Ok(())
    }
}

impl Drop for CffiServiceServer {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe { (self.vtable.destroy_service_server)(&mut view) };
        }
    }
}

// ============================================================================
// CffiServiceClient
// ============================================================================

/// Service client backed by a C vtable.
pub struct CffiServiceClient {
    vtable: &'static NrosRmwVtable,
    service_name_buf: [u8; NAME_BUF_LEN],
    type_name_buf: [u8; NAME_BUF_LEN],
    backend_data: *mut c_void,
    /// Stored request for blocking fallback in `try_recv_reply_raw`
    pending_request: [u8; 4096],
    /// Length of stored pending request (0 = no pending request)
    pending_len: usize,
}

impl CffiServiceClient {
    fn make_view(&mut self) -> NrosRmwServiceClient {
        NrosRmwServiceClient {
            service_name: self.service_name_buf.as_ptr(),
            type_name: self.type_name_buf.as_ptr(),
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

impl ServiceClientTrait for CffiServiceClient {
    type Error = TransportError;

    #[allow(deprecated)]
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, TransportError> {
        let mut view = self.make_view();
        let rc = unsafe {
            (self.vtable.call_raw)(
                &mut view,
                request.as_ptr(),
                request.len(),
                reply_buf.as_mut_ptr(),
                reply_buf.len(),
            )
        };
        if rc < 0 {
            return Err(error_from_ret(rc));
        }
        Ok(rc as usize)
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), TransportError> {
        if request.len() > self.pending_request.len() {
            return Err(TransportError::BufferTooSmall);
        }
        self.pending_request[..request.len()].copy_from_slice(request);
        self.pending_len = request.len();
        Ok(())
    }

    fn try_recv_reply_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        if self.pending_len == 0 {
            return Ok(None);
        }
        // Blocking fallback: copy request to stack, then call_raw
        let mut req_copy = [0u8; 4096];
        let req_len = self.pending_len;
        req_copy[..req_len].copy_from_slice(&self.pending_request[..req_len]);
        self.pending_len = 0;
        #[allow(deprecated)]
        let len = self.call_raw(&req_copy[..req_len], reply_buf)?;
        Ok(Some(len))
    }
}

impl Drop for CffiServiceClient {
    fn drop(&mut self) {
        if !self.backend_data.is_null() {
            let mut view = self.make_view();
            unsafe { (self.vtable.destroy_service_client)(&mut view) };
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
    unsafe fn copy_cstr(src: *const u8, dst: &mut [u8]) {
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

    unsafe extern "C" fn stub_open(
        _locator: *const u8,
        _mode: u8,
        _domain_id: u32,
        _node_name: *const u8,
        out: *mut NrosRmwSession,
    ) -> NrosRmwRet {
        unsafe {
            *(&raw mut STUB_OPEN_CALLED) = true;
            (*out).backend_data = 0xDEAD_BEEFusize as *mut c_void;
        }
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_close(_session: *mut NrosRmwSession) -> NrosRmwRet {
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
        _topic_name: *const u8,
        _type_name: *const u8,
        _type_hash: *const u8,
        _domain_id: u32,
        qos: *const NrosRmwQos,
        out: *mut NrosRmwPublisher,
    ) -> NrosRmwRet {
        // Capture the typed-struct fields the runtime supplied.
        unsafe {
            *(&raw mut STUB_CREATE_PUB_CALLED) = true;
            copy_cstr((*out).topic_name, &mut *(&raw mut STUB_LAST_TOPIC_NAME));
            copy_cstr((*out).type_name, &mut *(&raw mut STUB_LAST_TYPE_NAME));
            *(&raw mut STUB_LAST_QOS) = *qos;
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
            *(&raw mut STUB_PUBLISH_CALLED) = true;
            assert_eq!((*publisher).backend_data as usize, 0xCAFE);
            let mut buf = [0u8; 64];
            copy_cstr((*publisher).topic_name, &mut buf);
            assert_eq!(&buf[..], &*(&raw const STUB_LAST_TOPIC_NAME));
        }
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_create_subscriber(
        _: *mut NrosRmwSession,
        _: *const u8,
        _: *const u8,
        _: *const u8,
        _: u32,
        _: *const NrosRmwQos,
        out: *mut NrosRmwSubscriber,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = 0x1usize as *mut c_void;
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_subscriber(_: *mut NrosRmwSubscriber) {}
    unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscriber, _: *mut u8, _: usize) -> i32 {
        0
    }
    unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscriber) -> i32 {
        0
    }

    unsafe extern "C" fn stub_create_service_server(
        _: *mut NrosRmwSession,
        _: *const u8,
        _: *const u8,
        _: *const u8,
        _: u32,
        out: *mut NrosRmwServiceServer,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = 0x1usize as *mut c_void;
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_service_server(_: *mut NrosRmwServiceServer) {}
    unsafe extern "C" fn stub_try_recv_request(
        _: *mut NrosRmwServiceServer,
        _: *mut u8,
        _: usize,
        _: *mut i64,
    ) -> i32 {
        0
    }
    unsafe extern "C" fn stub_has_request(_: *mut NrosRmwServiceServer) -> i32 {
        0
    }
    unsafe extern "C" fn stub_send_reply(
        _: *mut NrosRmwServiceServer,
        _: i64,
        _: *const u8,
        _: usize,
    ) -> NrosRmwRet {
        NROS_RMW_RET_OK
    }

    unsafe extern "C" fn stub_create_service_client(
        _: *mut NrosRmwSession,
        _: *const u8,
        _: *const u8,
        _: *const u8,
        _: u32,
        out: *mut NrosRmwServiceClient,
    ) -> NrosRmwRet {
        unsafe {
            (*out).backend_data = 0x1usize as *mut c_void;
        }
        NROS_RMW_RET_OK
    }
    unsafe extern "C" fn stub_destroy_service_client(_: *mut NrosRmwServiceClient) {}
    unsafe extern "C" fn stub_call_raw(
        _: *mut NrosRmwServiceClient,
        _: *const u8,
        _: usize,
        _: *mut u8,
        _: usize,
    ) -> i32 {
        0
    }

    unsafe extern "C" fn stub_register_subscriber_event(
        _: *mut NrosRmwSubscriber,
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
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        register_subscriber_event: stub_register_subscriber_event,
        register_publisher_event: stub_register_publisher_event,
        assert_publisher_liveliness: stub_assert_publisher_liveliness,
        next_deadline_ms: None,
    };

    #[test]
    fn typed_struct_roundtrip() {
        // Register the stub vtable.
        let ret = unsafe { nros_rmw_cffi_register(&STUB_VTABLE) };
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
        let mut session = CffiRmw.open(&cfg).expect("session open");
        assert!(unsafe { *(&raw const STUB_OPEN_CALLED) });
        assert_eq!(session.node_name(), "test_node");

        // Create a publisher; verify backend received the typed
        // struct with topic_name + qos populated.
        let topic = TopicInfo::new("/chatter", "std_msgs/msg/Int32", "RIHS01_abc");
        let qos = nros_rmw::QosSettings::default();
        let publisher = session
            .create_publisher(&topic, qos)
            .expect("publisher create");
        assert!(unsafe { *(&raw const STUB_CREATE_PUB_CALLED) });
        let topic_buf = unsafe { &*(&raw const STUB_LAST_TOPIC_NAME) };
        assert_eq!(
            core::str::from_utf8(topic_buf)
                .unwrap_or("")
                .trim_end_matches('\0'),
            "/chatter"
        );
        let type_buf = unsafe { &*(&raw const STUB_LAST_TYPE_NAME) };
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
        assert!(unsafe { *(&raw const STUB_PUBLISH_CALLED) });
    }
}
