//! Node API for nros C API.
//!
//! A node is the main entity in ROS 2 that can have publishers, subscribers,
//! services, and other communication primitives.

use core::ffi::c_char;
use core::ptr;

use crate::constants::{MAX_NAME_LEN, MAX_NAMESPACE_LEN};
use crate::error::*;
use crate::support::{nros_support_state_t, nros_support_t};

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
    name: [u8; MAX_NAME_LEN],
    /// Node name length
    name_len: usize,
    /// Namespace storage
    namespace: [u8; MAX_NAMESPACE_LEN],
    /// Namespace length
    namespace_len: usize,
    /// Pointer to parent support context
    support: *const nros_support_t,
    /// Opaque pointer to internal Rust node
    _internal: *mut core::ffi::c_void,
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
            _internal: ptr::null_mut(),
        }
    }
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
    // Validate arguments
    if node.is_null() || support.is_null() || name.is_null() || namespace_.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let node = &mut *node;
    let support_ref = &*support;

    // Check if node is already initialized
    if node.state != nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Copy name
    let name_ptr = name as *const u8;
    let mut len = 0usize;
    while len < MAX_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        node.name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    node.name[len] = 0;
    node.name_len = len;

    // Copy namespace
    let ns_ptr = namespace_ as *const u8;
    len = 0;
    while len < MAX_NAMESPACE_LEN - 1 {
        let c = *ns_ptr.add(len);
        if c == 0 {
            break;
        }
        node.namespace[len] = c;
        len += 1;
    }
    node.namespace[len] = 0;
    node.namespace_len = len;

    // Store support reference
    node.support = support;

    // For now, we don't create an internal Rust node object
    // The node is just metadata; publishers/subscribers will use support directly
    node._internal = ptr::null_mut();
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

    // Clean up internal resources if any
    if !node._internal.is_null() {
        // Currently no internal resources to clean up
        node._internal = ptr::null_mut();
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
        assert_eq!(
            node.state,
            nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED
        );
        assert!(node.support.is_null());
        assert!(node._internal.is_null());
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
