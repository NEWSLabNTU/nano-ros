//! nros C++ API — header-only C++14 library + Rust FFI staticlib.
//!
//! This crate provides `extern "C"` functions designed for the nros-cpp
//! C++ headers. Unlike `nros-c` (which erases types into opaque handles),
//! `nros-cpp` preserves type information through the FFI boundary — each
//! message/service/action type gets its own FFI function.
//!
//! # Architecture
//!
//! ```text
//! C++ (nros-cpp headers)  →  extern "C"  →  nros-cpp (Rust)  →  nros-node
//! ```
//!
//! The C++ side provides inline opaque storage for all entity handles
//! (publisher, subscription, service, guard condition, executor, action).
//! No heap allocation required — fully alloc-free.
//!
//! All serialization/deserialization happens on the Rust side.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(dead_code)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "panic-halt")]
use panic_halt as _;

// FreeRTOS global allocator: wraps pvPortMalloc/vPortFree for alloc on no_std.
// FreeRTOS heap_4 returns 8-byte aligned pointers, sufficient for all nros types.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-freertos"))]
mod freertos_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn pvPortMalloc(size: u32) -> *mut core::ffi::c_void;
        fn vPortFree(ptr: *mut core::ffi::c_void);
    }

    struct FreeRtosAllocator;

    unsafe impl GlobalAlloc for FreeRtosAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { pvPortMalloc(layout.size() as u32) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { vPortFree(ptr as *mut core::ffi::c_void) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: FreeRtosAllocator = FreeRtosAllocator;
}

// ThreadX global allocator: wraps z_malloc/z_free which delegate to
// tx_byte_allocate/tx_byte_release via nros-platform-threadx.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-threadx"))]
mod threadx_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn z_malloc(size: usize) -> *mut core::ffi::c_void;
        fn z_free(ptr: *mut core::ffi::c_void);
    }

    struct ThreadXAllocator;

    unsafe impl GlobalAlloc for ThreadXAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { z_malloc(layout.size()) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { z_free(ptr as *mut core::ffi::c_void) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: ThreadXAllocator = ThreadXAllocator;
}

use core::ffi::{c_char, c_int, c_void};

// ── Core entity modules (alloc-free — caller provides inline storage) ──
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod guard_condition;
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod publisher;
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod service;
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod subscription;
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod timer;

// ── Action module (alloc-free — caller provides inline storage) ──
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod action;

// ============================================================================
// Error codes (mirror nros-c for consistency)
// ============================================================================

/// Return type for nros C++ FFI functions.
pub type nros_cpp_ret_t = c_int;

/// Success.
pub const NROS_CPP_RET_OK: nros_cpp_ret_t = 0;
/// Generic error.
pub const NROS_CPP_RET_ERROR: nros_cpp_ret_t = -1;
/// Timeout.
pub const NROS_CPP_RET_TIMEOUT: nros_cpp_ret_t = -2;
/// Invalid argument.
pub const NROS_CPP_RET_INVALID_ARGUMENT: nros_cpp_ret_t = -3;
/// Not initialized.
pub const NROS_CPP_RET_NOT_INIT: nros_cpp_ret_t = -4;
/// Resource limit reached.
pub const NROS_CPP_RET_FULL: nros_cpp_ret_t = -5;
/// Try again — operation not ready yet.
pub const NROS_CPP_RET_TRY_AGAIN: nros_cpp_ret_t = -6;
/// Reentrant call detected — executor is already spinning.
pub const NROS_CPP_RET_REENTRANT: nros_cpp_ret_t = -7;
/// Transport / connection error.
pub const NROS_CPP_RET_TRANSPORT_ERROR: nros_cpp_ret_t = -100;

// ============================================================================
// Inline opaque storage sizes (in u64 units)
// ============================================================================
//
// These constants define the inline storage for internal C++ FFI wrapper
// structs (CppPublisher, CppSubscription, etc.). The C++ side allocates
// buffers of this size; the Rust side writes directly into them.
// Compile-time assertions in each module verify the storage is large enough.

// Opaque storage sizes computed from size_of at compile time — always exact.
// When no RMW backend is enabled (workspace-level check), placeholder values
// are used. The placeholders are never used at runtime.

const fn u64s_for<T>() -> usize {
    core::mem::size_of::<T>().div_ceil(8)
}

// With RMW backend: exact sizes from actual types.
// Phase 87.6: `CppPublisher` removed — the FFI stores an `RmwPublisher`
// handle directly, sized via `NROS_PUBLISHER_SIZE` from the `nros` probe
// (see packages/core/nros-cpp/build.rs).
// Phase 87.6: `CppSubscription` removed — the FFI stores an
// `RmwSubscriber` handle directly, sized via `NROS_SUBSCRIBER_SIZE` from
// the `nros` probe.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const CPP_SERVICE_SERVER_OPAQUE_U64S: usize = u64s_for::<service::CppServiceServer>();
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const CPP_SERVICE_CLIENT_OPAQUE_U64S: usize = u64s_for::<service::CppServiceClient>();
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const CPP_ACTION_SERVER_OPAQUE_U64S: usize = u64s_for::<action::CppActionServer>();
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const CPP_ACTION_CLIENT_OPAQUE_U64S: usize = u64s_for::<action::CppActionClient>();

// Without RMW backend: placeholders for workspace-level check.
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const CPP_SERVICE_SERVER_OPAQUE_U64S: usize = 1;
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const CPP_SERVICE_CLIENT_OPAQUE_U64S: usize = 1;
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const CPP_ACTION_SERVER_OPAQUE_U64S: usize = 1;
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const CPP_ACTION_CLIENT_OPAQUE_U64S: usize = 1;

/// Inline storage for `GuardConditionHandle` (in u64 units).
pub const CPP_GUARD_HANDLE_OPAQUE_U64S: usize = u64s_for::<nros_node::GuardConditionHandle>();

// ============================================================================
// QoS types (passed from C++ to Rust by value)
// ============================================================================

/// QoS reliability policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_reliability_t {
    NROS_CPP_QOS_RELIABLE = 0,
    NROS_CPP_QOS_BEST_EFFORT = 1,
}

/// QoS durability policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_durability_t {
    NROS_CPP_QOS_VOLATILE = 0,
    NROS_CPP_QOS_TRANSIENT_LOCAL = 1,
}

/// QoS history policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_history_t {
    NROS_CPP_QOS_KEEP_LAST = 0,
    NROS_CPP_QOS_KEEP_ALL = 1,
}

/// QoS settings (passed by value from C++).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_qos_t {
    pub reliability: nros_cpp_qos_reliability_t,
    pub durability: nros_cpp_qos_durability_t,
    pub history: nros_cpp_qos_history_t,
    pub depth: c_int,
}

impl nros_cpp_qos_t {
    pub(crate) fn to_qos_settings(self) -> nros_rmw::QosSettings {
        use nros_rmw::{QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy};

        nros_rmw::QosSettings {
            reliability: match self.reliability {
                nros_cpp_qos_reliability_t::NROS_CPP_QOS_RELIABLE => QosReliabilityPolicy::Reliable,
                nros_cpp_qos_reliability_t::NROS_CPP_QOS_BEST_EFFORT => {
                    QosReliabilityPolicy::BestEffort
                }
            },
            durability: match self.durability {
                nros_cpp_qos_durability_t::NROS_CPP_QOS_VOLATILE => QosDurabilityPolicy::Volatile,
                nros_cpp_qos_durability_t::NROS_CPP_QOS_TRANSIENT_LOCAL => {
                    QosDurabilityPolicy::TransientLocal
                }
            },
            history: match self.history {
                nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_LAST => QosHistoryPolicy::KeepLast,
                nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_ALL => QosHistoryPolicy::KeepAll,
            },
            depth: self.depth as u32,
        }
    }
}

// ============================================================================
// Build-time configuration
// ============================================================================

mod executor_config {
    include!(concat!(env!("OUT_DIR"), "/nros_cpp_ffi_config.rs"));
}
pub use executor_config::CPP_EXECUTOR_OPAQUE_U64S;

// Compile-time asserts that the auto-generated C-side STORAGE macros
// are large enough for their Rust counterparts. If a Rust type grows
// past the estimate emitted by build.rs, compilation fails with a
// clear error instead of silently overflowing caller-provided storage.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
const _: () = {
    // Phase 87.6: `CppPublisher` and `CppSubscription` assertions removed —
    // both now use thin-wrapper storage sized from the Rust SSoT
    // (`NROS_PUBLISHER_SIZE` / `NROS_SUBSCRIBER_SIZE`).
    assert!(
        core::mem::size_of::<service::CppServiceServer>()
            <= executor_config::CPP_SERVICE_STORAGE_BYTES,
        "NROS_CPP_SERVICE_SERVER_STORAGE_SIZE too small for CppServiceServer — bump service_bytes in build.rs"
    );
    assert!(
        core::mem::size_of::<service::CppServiceClient>()
            <= executor_config::CPP_SERVICE_STORAGE_BYTES,
        "NROS_CPP_SERVICE_CLIENT_STORAGE_SIZE too small for CppServiceClient — bump service_bytes in build.rs"
    );
    assert!(
        core::mem::size_of::<nros_node::GuardConditionHandle>()
            <= executor_config::CPP_GUARD_STORAGE_BYTES,
        "NROS_CPP_GUARD_CONDITION_STORAGE_SIZE too small for GuardConditionHandle — bump guard_bytes in build.rs"
    );
};

// ============================================================================
// Executor handle (alloc-free — caller provides inline storage)
// ============================================================================

/// The concrete nros-node executor type used by the C++ FFI.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub(crate) type CppExecutor = nros_node::Executor;

/// Context wrapping the executor and the domain ID.
///
/// The executor doesn't store domain_id itself — it's consumed during
/// session open. We keep it here so publisher/subscription creation
/// can pass the correct value to `TopicInfo::with_domain()`.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub(crate) struct CppContext {
    pub(crate) executor: CppExecutor,
    pub(crate) domain_id: u32,
}

// Compile-time assertion: inline storage must fit CppContext.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
const _: () = assert!(
    core::mem::size_of::<CppContext>() <= CPP_EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_EXECUTOR_OPAQUE_U64S too small for CppContext — increase NROS_EXECUTOR_ARENA_SIZE \
     or NROS_EXECUTOR_MAX_CBS, or adjust the overhead in build.rs"
);

// ============================================================================
// Init / Fini
// ============================================================================

/// Initialize an nros executor session.
///
/// Opens a middleware connection and writes the executor context directly
/// into caller-provided storage (no heap allocation).
///
/// # Parameters
/// * `locator` — Middleware locator (e.g., `"tcp/127.0.0.1:7447"`), or NULL for default.
/// * `domain_id` — ROS domain ID (0–232).
/// * `node_name` — Node name (null-terminated string). Must not be NULL.
/// * `namespace` — Node namespace (null-terminated string), or NULL for `"/"`.
/// * `storage` — Pointer to caller-provided storage (at least `CPP_EXECUTOR_OPAQUE_U64S * 8` bytes,
///   aligned to 8 bytes). The executor is written directly into this buffer.
///
/// # Safety
/// * `node_name` must be a valid null-terminated string.
/// * `locator` and `namespace` must be valid null-terminated strings or NULL.
/// * `storage` must be a valid pointer to appropriately sized and aligned storage.
///
/// # Returns
/// `NROS_CPP_RET_OK` on success, error code otherwise.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_init(
    locator: *const c_char,
    domain_id: u8,
    node_name: *const c_char,
    namespace: *const c_char,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node_name.is_null() || storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_name_str = match unsafe { cstr_to_str(node_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ns_str = if namespace.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    let locator_str = if locator.is_null() {
        "tcp/127.0.0.1:7447"
    } else {
        match unsafe { cstr_to_str(locator) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    let config = nros_node::ExecutorConfig::new(locator_str)
        .domain_id(domain_id as u32)
        .node_name(node_name_str)
        .namespace(ns_str);

    match CppExecutor::open(&config) {
        Ok(executor) => {
            let ctx = CppContext {
                executor,
                domain_id: domain_id as u32,
            };
            // Write directly into caller-provided storage — no heap allocation.
            unsafe { core::ptr::write(storage as *mut CppContext, ctx) };
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Shut down an nros executor session.
///
/// Drops the executor in-place within the caller's storage.
///
/// # Safety
/// `storage` must point to a live `CppContext` written by `nros_cpp_init()`, or NULL (no-op).
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_fini(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }

    unsafe {
        let ctx = &mut *(storage as *mut CppContext);
        let _ = ctx.executor.close();
        core::ptr::drop_in_place(storage as *mut CppContext);
    }

    NROS_CPP_RET_OK
}

// ============================================================================
// Node
// ============================================================================

/// Opaque node handle.
///
/// A node is a lightweight view into the executor. In Rust, `Node<'_, S>` is
/// a borrow of the executor. For the C++ FFI we store the executor pointer
/// plus the node name/namespace, and re-create the borrow when needed.
#[repr(C)]
pub struct nros_cpp_node_t {
    /// Pointer to the parent executor handle (not owned).
    pub executor: *mut c_void,
    /// Node name (null-terminated, max 64 bytes including null).
    pub name: [u8; 64],
    /// Node namespace (null-terminated, max 64 bytes including null).
    pub namespace: [u8; 64],
}

/// Create a node on an executor.
///
/// # Parameters
/// * `executor_handle` — Opaque executor handle from `nros_cpp_init()`.
/// * `name` — Node name (null-terminated). Must not be NULL.
/// * `namespace` — Node namespace (null-terminated), or NULL for `"/"`.
/// * `out_node` — Receives the node handle on success.
///
/// # Safety
/// * `executor_handle` must be a valid handle from `nros_cpp_init()`.
/// * `name` must be a valid null-terminated string.
/// * `namespace` must be a valid null-terminated string or NULL.
/// * `out_node` must be a valid pointer to an `nros_cpp_node_t`.
///
/// # Returns
/// `NROS_CPP_RET_OK` on success, error code otherwise.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_create(
    executor_handle: *mut c_void,
    name: *const c_char,
    namespace: *const c_char,
    out_node: *mut nros_cpp_node_t,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || name.is_null() || out_node.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    if name_str.len() >= 64 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ns_str = if namespace.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace) } {
            Some(s) if s.len() < 64 => s,
            _ => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    // Verify the executor handle is valid by trying to create a node.
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    match ctx.executor.create_node(name_str) {
        Ok(_node) => {
            // The node is a borrow — we can't store it across FFI.
            // Instead, store the executor pointer + name/namespace so
            // we can re-create the borrow in future calls.
            let out = unsafe { &mut *out_node };
            out.executor = executor_handle;

            // Copy name
            out.name = [0u8; 64];
            out.name[..name_str.len()].copy_from_slice(name_str.as_bytes());

            // Copy namespace
            out.namespace = [0u8; 64];
            out.namespace[..ns_str.len()].copy_from_slice(ns_str.as_bytes());

            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy a node.
///
/// Currently a no-op since the node is just metadata referencing the executor.
/// The executor owns all resources.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_node_destroy(_node: *mut nros_cpp_node_t) -> nros_cpp_ret_t {
    // Node is a lightweight view — nothing to free.
    NROS_CPP_RET_OK
}

/// Get the node name.
///
/// Returns a pointer to the null-terminated name string stored in the node handle.
/// The pointer is valid as long as the `nros_cpp_node_t` is alive.
///
/// # Safety
/// `node` must be a valid pointer to an initialized `nros_cpp_node_t`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_name(node: *const nros_cpp_node_t) -> *const c_char {
    if node.is_null() {
        return core::ptr::null();
    }
    unsafe { (*node).name.as_ptr() as *const c_char }
}

/// Get the node namespace.
///
/// Returns a pointer to the null-terminated namespace string stored in the node handle.
/// The pointer is valid as long as the `nros_cpp_node_t` is alive.
///
/// # Safety
/// `node` must be a valid pointer to an initialized `nros_cpp_node_t`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_namespace(
    node: *const nros_cpp_node_t,
) -> *const c_char {
    if node.is_null() {
        return core::ptr::null();
    }
    unsafe { (*node).namespace.as_ptr() as *const c_char }
}

// ============================================================================
// Spin
// ============================================================================

/// Drive transport I/O and dispatch any registered callbacks.
///
/// Call this periodically so subscriptions can receive data.
///
/// # Parameters
/// * `handle` — Opaque executor handle from `nros_cpp_init()`.
/// * `timeout_ms` — Maximum time to block waiting for I/O (milliseconds).
///
/// # Safety
/// `handle` must be a valid handle returned by `nros_cpp_init()`.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_spin_once(
    handle: *mut c_void,
    timeout_ms: i32,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let ms = timeout_ms.max(0) as u64;
    let _ = ctx
        .executor
        .spin_once(core::time::Duration::from_millis(ms));
    NROS_CPP_RET_OK
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a C null-terminated string to a Rust `&str`.
///
/// Returns `None` if the pointer is null or the bytes are not valid UTF-8.
pub(crate) unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    // Find null terminator
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 4096 {
                return None; // safety bound
            }
        }
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok()
}
