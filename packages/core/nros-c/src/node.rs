//! Node API for nros C API.
//!
//! A node is the main entity in ROS 2 that can have publishers, subscribers,
//! services, and other communication primitives.

use core::{ffi::c_char, ptr};

use crate::{
    constants::{MAX_LOCATOR_LEN, MAX_NAME_LEN, MAX_NAMESPACE_LEN, MAX_RMW_NAME_LEN},
    error::*,
    support::{nros_support_state_t, nros_support_t},
};

/// Sentinel value for `domain_id_override`. When set, the support context's
/// domain_id is used instead of the per-Node override.
pub const NROS_DOMAIN_ID_INHERIT: u32 = u32::MAX;

/// Node state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_node_state_t {
    /// Not initialized
    NROS_NODE_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_NODE_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_NODE_STATE_SHUTDOWN = 2,
}

/// Node structure.
///
/// Represents a ROS 2 node with a name and namespace.
#[repr(C)]
pub struct nros_node_t {
    /// Current state
    pub state: nros_node_state_t,
    /// Node name storage
    pub name: [u8; MAX_NAME_LEN],
    /// Node name length
    pub name_len: usize,
    /// Namespace storage
    pub namespace: [u8; MAX_NAMESPACE_LEN],
    /// Namespace length
    pub namespace_len: usize,
    /// Pointer to parent support context
    pub support: *const nros_support_t,

    // Phase 104.C.8 — multi-RMW + per-Node SchedContext fields. Populated
    // by `nros_node_init_ex` from `nros_node_options_t`. Zero values mean
    // "inherit from the support context / executor default" so the legacy
    // `nros_node_init(node, support, name, ns)` entry point keeps its old
    // single-Node behaviour through `nros_node_init_ex` with default
    // options.
    /// RMW backend name (UTF-8, NUL-terminated within `rmw_name_len`).
    /// Empty (`rmw_name_len == 0`) selects the first-registered backend.
    pub rmw_name: [u8; MAX_RMW_NAME_LEN],
    /// Length of `rmw_name` in bytes (excluding NUL). 0 = inherit.
    pub rmw_name_len: usize,
    /// Per-Node domain ID. `NROS_DOMAIN_ID_INHERIT` (== u32::MAX) means
    /// "use the support context's domain_id".
    pub domain_id_override: u32,
    /// SchedContext slot to inherit on every handle created by this Node
    /// (Phase 104.C.4). 0 = inherit the executor's default Fifo context.
    pub sched_context_id: u8,
    /// Reserved for future use (alignment + ABI stability).
    pub _reserved: [u8; 3],
    /// Opaque NodeId slot returned by `Executor::node_builder(...).build()`
    /// when this Node is bound to an Executor. 0 = primary Node (legacy
    /// single-Node path). Internal use only — readers should treat as
    /// opaque.
    pub node_id: u8,
}

impl Default for nros_node_t {
    fn default() -> Self {
        Self {
            state: nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED,
            name: [0u8; MAX_NAME_LEN],
            name_len: 0,
            namespace: [0u8; MAX_NAMESPACE_LEN],
            namespace_len: 0,
            support: ptr::null(),
            rmw_name: [0u8; MAX_RMW_NAME_LEN],
            rmw_name_len: 0,
            domain_id_override: NROS_DOMAIN_ID_INHERIT,
            sched_context_id: 0,
            _reserved: [0u8; 3],
            node_id: 0,
        }
    }
}

/// Phase 104.C.8 — extended node-creation options.
///
/// Mirrors the Rust `Executor::node_builder(name).rmw(rmw_name).
/// locator(...).domain_id(...).namespace(...).sched(...)` chain. Pass an
/// instance to [`nros_node_init_ex`] to bind a Node to a specific RMW
/// backend, locator, domain, and default SchedContext. Zero fields keep
/// the legacy single-Node single-backend behaviour for back-compat
/// callers.
///
/// The struct contains plain inline buffers — no pointer fields — so it
/// is safe to stack-allocate, memcpy, and pass across the FFI.
#[repr(C)]
pub struct nros_node_options_t {
    /// Namespace storage (UTF-8, NUL-terminated within `namespace_len`).
    pub namespace: [u8; MAX_NAMESPACE_LEN],
    /// Length of `namespace` in bytes (excluding NUL).
    pub namespace_len: usize,
    /// RMW backend name (e.g. "zenoh", "dds"). Empty selects first-
    /// registered (single-backend convenience).
    pub rmw_name: [u8; MAX_RMW_NAME_LEN],
    /// Length of `rmw_name`.
    pub rmw_name_len: usize,
    /// Optional per-Node locator override (`tcp/...`, `udp/...`, …).
    /// Empty inherits the support context's locator.
    pub locator: [u8; MAX_LOCATOR_LEN],
    /// Length of `locator`.
    pub locator_len: usize,
    /// Per-Node domain ID. `NROS_DOMAIN_ID_INHERIT` = inherit support's.
    pub domain_id_override: u32,
    /// SchedContext slot for handle inheritance. 0 = executor default.
    pub sched_context_id: u8,
    /// Reserved for future use; must be zero.
    pub _reserved: [u8; 3],
}

impl Default for nros_node_options_t {
    fn default() -> Self {
        Self {
            namespace: [0u8; MAX_NAMESPACE_LEN],
            namespace_len: 0,
            rmw_name: [0u8; MAX_RMW_NAME_LEN],
            rmw_name_len: 0,
            locator: [0u8; MAX_LOCATOR_LEN],
            locator_len: 0,
            domain_id_override: NROS_DOMAIN_ID_INHERIT,
            sched_context_id: 0,
            _reserved: [0u8; 3],
        }
    }
}

/// Get a zero-initialised `nros_node_options_t`.
///
/// All fields default to "inherit" — `rmw_name_len = 0`, `locator_len = 0`,
/// `domain_id_override = NROS_DOMAIN_ID_INHERIT`, `sched_context_id = 0`.
/// Callers populate only the fields they want to override.
#[unsafe(no_mangle)]
pub extern "C" fn nros_node_get_default_options() -> nros_node_options_t {
    nros_node_options_t::default()
}

/// Get a zero-initialized node.
///
/// # Safety
/// Returns a stack-allocated struct that must be initialized before use.
#[unsafe(no_mangle)]
pub extern "C" fn nros_node_get_zero_initialized() -> nros_node_t {
    nros_node_t::default()
}

/// Initialize a node with default options.
///
/// Equivalent to building a [`nros_node_options_t`] via
/// [`nros_node_get_default_options`], copying `namespace_` into its
/// `namespace` field, and calling [`nros_node_init_ex`]. The shim is
/// kept for source-compatibility with rclc-style callers that pre-date
/// Phase 104.C.8.
///
/// # Parameters
/// * `node` - Pointer to a zero-initialized node
/// * `support` - Pointer to an initialized support context
/// * `name` - Node name (null-terminated string)
/// * `namespace_` - Node namespace (null-terminated string, use "/" for root)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or strings are invalid
/// * `NROS_RET_NOT_INIT` if support is not initialized
/// * `NROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All pointers must be valid
/// * `name` and `namespace_` must be valid null-terminated strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_init(
    node: *mut nros_node_t,
    support: *const nros_support_t,
    name: *const c_char,
    namespace_: *const c_char,
) -> nros_ret_t {
    if namespace_.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    let mut options = nros_node_options_t::default();
    options.namespace_len = crate::util::copy_cstr_into(namespace_, &mut options.namespace);
    unsafe { nros_node_init_ex(node, support, name, &options) }
}

/// Phase 104.C.8 — initialize a Node with extended options.
///
/// Thin C wrapper over the Rust `Executor::node_builder(name).rmw(...)
/// .locator(...).domain_id(...).namespace(...).sched(...).build()`
/// chain. Options fields with `*_len == 0` (or `domain_id_override ==
/// NROS_DOMAIN_ID_INHERIT`) inherit from the support context, matching
/// the legacy single-Node behaviour `nros_node_init` provides.
///
/// The `rmw_name` selector drives Phase 104 multi-RMW Node binding: a
/// bridge node can be initialised with `options.rmw_name = "dds"` while
/// the support context's primary backend is `"zenoh"`, and subsequent
/// publishers/subscribers created via this Node route through the named
/// backend's session. (Internal multi-Session dispatch piggy-backs on
/// the executor's `extra_sessions` cache; see Phase 104.C.3.)
///
/// Currently the inline `node_id` slot stays 0; per-Node multi-RMW
/// dispatch in C lands once the C executor surfaces
/// `Executor::node_builder` (Phase 104.C.8 follow-up). Options fields
/// round-trip into the node struct today so users can write code
/// against the final API surface without waiting for that follow-up.
///
/// # Parameters
/// * `node` - Pointer to a zero-initialized node
/// * `support` - Pointer to an initialized support context
/// * `name` - Node name (null-terminated string)
/// * `options` - Pointer to an [`nros_node_options_t`] (must be non-NULL)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` on NULL / invalid strings / overrun buffers
/// * `NROS_RET_BAD_SEQUENCE` if the node is already initialized
/// * `NROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All pointers must be valid
/// * `name` must be a valid NUL-terminated UTF-8 string
/// * `options` fields must satisfy their declared length invariants
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_init_ex(
    node: *mut nros_node_t,
    support: *const nros_support_t,
    name: *const c_char,
    options: *const nros_node_options_t,
) -> nros_ret_t {
    if node.is_null() || support.is_null() || name.is_null() || options.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let node = &mut *node;
    let support_ref = &*support;
    let opts = &*options;

    if node.state != nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }
    if support_ref.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Copy name (required — empty rejected).
    node.name_len = crate::util::copy_cstr_into(name, &mut node.name);
    if node.name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Validate options length fields against their buffer caps.
    if opts.namespace_len > MAX_NAMESPACE_LEN
        || opts.rmw_name_len > MAX_RMW_NAME_LEN
        || opts.locator_len > MAX_LOCATOR_LEN
    {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Mirror namespace from options into the node.
    node.namespace[..opts.namespace_len].copy_from_slice(&opts.namespace[..opts.namespace_len]);
    node.namespace_len = opts.namespace_len;

    // Mirror multi-RMW + SchedContext metadata.
    node.rmw_name[..opts.rmw_name_len].copy_from_slice(&opts.rmw_name[..opts.rmw_name_len]);
    node.rmw_name_len = opts.rmw_name_len;
    node.domain_id_override = opts.domain_id_override;
    node.sched_context_id = opts.sched_context_id;

    // `node_id` stays 0 for the legacy single-Node path. Future
    // follow-up (Phase 104.C.8.b) will call into the Executor's
    // `node_builder(...).build()` and store the returned NodeId
    // here when the C executor exposes a stable factory entry.

    node.support = support;
    node.state = nros_node_state_t::NROS_NODE_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Finalize a node.
///
/// # Parameters
/// * `node` - Pointer to an initialized node
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if node is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `node` must be a valid pointer to an initialized nros_node_t
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_fini(node: *mut nros_node_t) -> nros_ret_t {
    if node.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let node = &mut *node;

    if node.state != nros_node_state_t::NROS_NODE_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    node.support = ptr::null();
    node.state = nros_node_state_t::NROS_NODE_STATE_SHUTDOWN;

    NROS_RET_OK
}

/// Get the node name.
///
/// # Parameters
/// * `node` - Pointer to an initialized node
///
/// # Returns
/// * Pointer to the node name (null-terminated), or NULL if invalid
///
/// # Safety
/// * `node` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_get_name(node: *const nros_node_t) -> *const c_char {
    if node.is_null() {
        return ptr::null();
    }

    let node = &*node;
    if node.state != nros_node_state_t::NROS_NODE_STATE_INITIALIZED {
        return ptr::null();
    }

    node.name.as_ptr() as *const c_char
}

/// Get the node namespace.
///
/// # Parameters
/// * `node` - Pointer to an initialized node
///
/// # Returns
/// * Pointer to the node namespace (null-terminated), or NULL if invalid
///
/// # Safety
/// * `node` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_get_namespace(node: *const nros_node_t) -> *const c_char {
    if node.is_null() {
        return ptr::null();
    }

    let node = &*node;
    if node.state != nros_node_state_t::NROS_NODE_STATE_INITIALIZED {
        return ptr::null();
    }

    node.namespace.as_ptr() as *const c_char
}

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn node_init_null_ptrs() {
        let name = b"test\0";
        let ns = b"/\0";

        // NULL node → INVALID_ARGUMENT
        let mut support = crate::support::nros_support_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_node_init(
                    core::ptr::null_mut(),
                    &support,
                    name.as_ptr() as *const core::ffi::c_char,
                    ns.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL support → INVALID_ARGUMENT
        let mut node = nros_node_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_node_init(
                    &mut node,
                    core::ptr::null(),
                    name.as_ptr() as *const core::ffi::c_char,
                    ns.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL name → INVALID_ARGUMENT
        assert_eq!(
            unsafe {
                nros_node_init(
                    &mut node,
                    &support,
                    core::ptr::null(),
                    ns.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL namespace → INVALID_ARGUMENT
        assert_eq!(
            unsafe {
                nros_node_init(
                    &mut node,
                    &support,
                    name.as_ptr() as *const core::ffi::c_char,
                    core::ptr::null(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn node_zero_initialized_state() {
        let node = nros_node_get_zero_initialized();
        assert_eq!(node.state, nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED);
        assert!(node.support.is_null());
    }
}

impl nros_node_t {
    /// Get the node name as a string slice
    pub(crate) fn get_name_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }

    /// Get the namespace as a string slice
    pub(crate) fn get_namespace_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.namespace[..self.namespace_len]) }
    }

    /// Get the support context
    pub(crate) unsafe fn get_support(&self) -> Option<&nros_support_t> {
        if self.support.is_null() {
            None
        } else {
            Some(&*self.support)
        }
    }

    /// Get the support context mutably
    ///
    /// This returns a mutable reference from an immutable pointer, which is
    /// intentional for C FFI where the node stores a const pointer but the
    /// support may need to be mutated.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn get_support_mut(&self) -> Option<&mut nros_support_t> {
        if self.support.is_null() {
            None
        } else {
            Some(&mut *(self.support as *mut nros_support_t))
        }
    }
}
