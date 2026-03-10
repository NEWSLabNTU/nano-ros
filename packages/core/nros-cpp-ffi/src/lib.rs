//! Typed C++ FFI bindings for nros.
//!
//! This crate provides `extern "C"` functions designed for the nros-cpp
//! header-only C++ library. Unlike `nros-c` (which erases types into opaque
//! handles), `nros-cpp-ffi` preserves type information through the FFI
//! boundary — each message/service/action type gets its own FFI function.
//!
//! # Architecture
//!
//! ```text
//! C++ (nros-cpp headers)  →  extern "C"  →  nros-cpp-ffi  →  nros-node
//! ```
//!
//! The C++ side holds opaque `void*` handles to Rust objects. All
//! serialization/deserialization happens on the Rust side.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(dead_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::ffi::{c_char, c_int, c_void};

#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
mod action;
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
mod publisher;
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
mod service;
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
mod subscription;

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
/// Transport / connection error.
pub const NROS_CPP_RET_TRANSPORT_ERROR: nros_cpp_ret_t = -100;

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
// Executor handle (opaque, boxed on Rust heap)
// ============================================================================

// The concrete executor type used by the C++ FFI.
//
// Uses the same env-var-driven defaults as nros-node:
// - MAX_CBS = NROS_EXECUTOR_MAX_CBS (default 4)
// - CB_ARENA = NROS_EXECUTOR_ARENA_SIZE (default 4096)
//
// These match what `Executor<_>` resolves to in Rust user code.
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
pub(crate) type CppExecutor = nros_node::Executor<nros::internals::RmwSession>;

/// Context wrapping the executor and the domain ID.
///
/// The executor doesn't store domain_id itself — it's consumed during
/// session open. We keep it here so publisher/subscription creation
/// can pass the correct value to `TopicInfo::with_domain()`.
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
pub(crate) struct CppContext {
    pub(crate) executor: CppExecutor,
    pub(crate) domain_id: u32,
}

// ============================================================================
// Init / Fini
// ============================================================================

/// Initialize an nros executor session.
///
/// Opens a middleware connection and returns an opaque executor handle.
/// The handle must be destroyed with `nros_cpp_fini()`.
///
/// # Parameters
/// * `locator` — Middleware locator (e.g., `"tcp/127.0.0.1:7447"`), or NULL for default.
/// * `domain_id` — ROS domain ID (0–232).
/// * `node_name` — Node name (null-terminated string). Must not be NULL.
/// * `namespace` — Node namespace (null-terminated string), or NULL for `"/"`.
/// * `out_handle` — Receives the opaque executor handle on success.
///
/// # Safety
/// * `node_name` must be a valid null-terminated string.
/// * `locator` and `namespace` must be valid null-terminated strings or NULL.
/// * `out_handle` must be a valid pointer to a `*mut c_void`.
///
/// # Returns
/// `NROS_CPP_RET_OK` on success, error code otherwise.
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_init(
    locator: *const c_char,
    domain_id: u8,
    node_name: *const c_char,
    namespace: *const c_char,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node_name.is_null() || out_handle.is_null() {
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
            let boxed = alloc::boxed::Box::new(ctx);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Shut down an nros executor session.
///
/// Closes the middleware connection and frees the executor handle.
///
/// # Safety
/// `handle` must be a valid handle returned by `nros_cpp_init()`, or NULL (no-op).
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_fini(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }

    unsafe {
        let mut ctx = alloc::boxed::Box::from_raw(handle as *mut CppContext);
        let _ = ctx.executor.close();
    }
    // context dropped here

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
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
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
#[cfg(all(
    feature = "alloc",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi")
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
    let _ = ctx.executor.spin_once(timeout_ms);
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
