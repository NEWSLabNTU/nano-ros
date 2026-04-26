//! Subscription API for nros C API.
//!
//! Subscriptions receive messages from topics that publishers send to.

use core::ffi::{c_char, c_void};
use core::ptr;

use crate::constants::{MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};
use crate::publisher::nros_message_type_t;
use crate::qos::nros_qos_t;

/// Subscription callback function type.
///
/// # Parameters
/// * `data` - Pointer to received CDR-serialized message data
/// * `len` - Length of data in bytes
/// * `context` - User-provided context pointer
pub type nros_subscription_callback_t =
    Option<unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void)>;

/// Subscription state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_subscription_state_t {
    /// Not initialized
    NROS_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
}

/// Subscription structure.
#[repr(C)]
pub struct nros_subscription_t {
    /// Current state
    pub state: nros_subscription_state_t,
    /// Topic name storage
    pub topic_name: [u8; MAX_TOPIC_LEN],
    /// Topic name length
    pub topic_name_len: usize,
    /// Type name storage
    pub type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    pub type_name_len: usize,
    /// Type hash storage
    pub type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    pub type_hash_len: usize,
    /// User callback function
    pub callback: nros_subscription_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// QoS settings (stored during init, used by executor registration)
    pub qos: crate::qos::nros_qos_t,
    /// Handle ID from executor registration (SIZE_MAX = not registered)
    pub handle_id: usize,
}

impl Default for nros_subscription_t {
    fn default() -> Self {
        Self {
            state: nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            qos: crate::qos::nros_qos_t::default(),
            handle_id: usize::MAX,
        }
    }
}

/// Get a zero-initialized subscription.
#[unsafe(no_mangle)]
pub extern "C" fn nros_subscription_get_zero_initialized() -> nros_subscription_t {
    nros_subscription_t::default()
}

/// Initialize a subscription with default QoS (RELIABLE, KEEP_LAST(10)).
///
/// This is the recommended initialization function for most use cases.
/// Uses `QOS_PROFILE_DEFAULT` which provides reliable delivery.
///
/// # Parameters
/// * `subscription` - Pointer to a zero-initialized subscription
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
/// * `callback` - Callback function to invoke when messages arrive
/// * `context` - User context pointer passed to callback (can be NULL)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NROS_RET_NOT_INIT` if node is not initialized
/// * `NROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_init(
    subscription: *mut nros_subscription_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
    callback: nros_subscription_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    nros_subscription_init_with_qos(
        subscription,
        node,
        type_info,
        topic_name,
        callback,
        context,
        ptr::null(),
    )
}

/// Initialize a subscription with custom QoS.
///
/// # Safety
/// See `nros_subscription_init` for safety requirements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_init_with_qos(
    subscription: *mut nros_subscription_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
    callback: nros_subscription_callback_t,
    context: *mut c_void,
    qos: *const nros_qos_t,
) -> nros_ret_t {
    validate_not_null!(subscription, node, type_info, topic_name);

    if callback.is_none() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let subscription = &mut *subscription;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        subscription,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );

    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy topic name (required — empty rejected)
    subscription.topic_name_len =
        crate::util::copy_cstr_into(topic_name, &mut subscription.topic_name);
    if subscription.topic_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    subscription.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut subscription.type_name);
    subscription.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut subscription.type_hash);

    // Store callback and context
    subscription.callback = callback;
    subscription.context = context;
    subscription.node = node;

    // Store QoS settings for later use by executor registration
    subscription.qos = if qos.is_null() {
        crate::qos::NROS_QOS_DEFAULT
    } else {
        *qos
    };

    // Subscriber creation is deferred to nros_executor_add_subscription(),
    // which calls nros_node::Executor::add_subscription_raw_with_qos_sized().
    subscription.handle_id = usize::MAX;
    subscription.state = nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Finalize a subscription.
///
/// # Parameters
/// * `subscription` - Pointer to an initialized subscription
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if subscription is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `subscription` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_fini(
    subscription: *mut nros_subscription_t,
) -> nros_ret_t {
    validate_not_null!(subscription);

    let subscription = &mut *subscription;

    validate_state!(
        subscription,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED
    );

    // The subscriber lives in the executor arena (if registered),
    // so we don't drop anything here — just reset metadata.
    subscription.handle_id = usize::MAX;
    subscription.callback = None;
    subscription.context = ptr::null_mut();
    subscription.node = ptr::null();
    subscription.state = nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_SHUTDOWN;

    NROS_RET_OK
}

/// Get the topic name of a subscription.
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_get_topic_name(
    subscription: *const nros_subscription_t,
) -> *const c_char {
    if subscription.is_null() {
        return ptr::null();
    }

    let subscription = &*subscription;
    if subscription.state != nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED {
        return ptr::null();
    }

    subscription.topic_name.as_ptr() as *const c_char
}

/// Check if subscription is valid (initialized).
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
///
/// # Returns
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_is_valid(
    subscription: *const nros_subscription_t,
) -> bool {
    if subscription.is_null() {
        return false;
    }

    let subscription = &*subscription;
    subscription.state == nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED
}

// Internal helper methods for executor
impl nros_subscription_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> nros_subscription_callback_t {
        self.callback
    }

    /// Get the user context
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Get the stored QoS as `nros_rmw::QosSettings`
    pub(crate) fn get_qos_settings(&self) -> nros_rmw::QosSettings {
        self.qos.to_qos_settings()
    }

    /// Set the handle ID from executor registration
    pub(crate) fn set_handle_id(&mut self, id: nros_node::HandleId) {
        self.handle_id = id.0;
    }
}
