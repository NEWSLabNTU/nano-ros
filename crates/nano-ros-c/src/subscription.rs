//! Subscription API for nano-ros C API.
//!
//! Subscriptions receive messages from topics that publishers send to.

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use crate::constants::{MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nano_ros_node_state_t, nano_ros_node_t};
use crate::publisher::nano_ros_message_type_t;
use crate::qos::nano_ros_qos_t;
use crate::support::nano_ros_support_state_t;

/// Subscription callback function type.
///
/// # Parameters
/// * `data` - Pointer to received CDR-serialized message data
/// * `len` - Length of data in bytes
/// * `context` - User-provided context pointer
pub type nano_ros_subscription_callback_t =
    Option<unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void)>;

/// Subscription state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_subscription_state_t {
    /// Not initialized
    NANO_ROS_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
}

/// Subscription structure.
#[repr(C)]
pub struct nano_ros_subscription_t {
    /// Current state
    pub state: nano_ros_subscription_state_t,
    /// Topic name storage
    topic_name: [u8; MAX_TOPIC_LEN],
    /// Topic name length
    topic_name_len: usize,
    /// Type name storage
    type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    type_name_len: usize,
    /// Type hash storage
    type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    type_hash_len: usize,
    /// User callback function
    callback: nano_ros_subscription_callback_t,
    /// User context pointer
    context: *mut c_void,
    /// Pointer to parent node
    node: *const nano_ros_node_t,
    /// Opaque pointer to internal Rust subscriber
    _internal: *mut c_void,
}

impl Default for nano_ros_subscription_t {
    fn default() -> Self {
        Self {
            state: nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

/// Get a zero-initialized subscription.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_subscription_get_zero_initialized() -> nano_ros_subscription_t {
    nano_ros_subscription_t::default()
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
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_init(
    subscription: *mut nano_ros_subscription_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
    callback: nano_ros_subscription_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    nano_ros_subscription_init_with_qos(
        subscription,
        node,
        type_info,
        topic_name,
        callback,
        context,
        ptr::null(),
    )
}

/// Initialize a subscription with default QoS (RELIABLE, KEEP_LAST(10)).
///
/// Alias for `nano_ros_subscription_init()` for rclc API compatibility.
///
/// # Safety
/// See `nano_ros_subscription_init()` for safety requirements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_init_default(
    subscription: *mut nano_ros_subscription_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
    callback: nano_ros_subscription_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    nano_ros_subscription_init_with_qos(
        subscription,
        node,
        type_info,
        topic_name,
        callback,
        context,
        ptr::null(),
    )
}

/// Initialize a subscription with best-effort QoS (BEST_EFFORT, VOLATILE).
///
/// Use this for sensor data or high-frequency topics where occasional
/// message loss is acceptable but low latency is preferred.
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
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_init_best_effort(
    subscription: *mut nano_ros_subscription_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
    callback: nano_ros_subscription_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    nano_ros_subscription_init_with_qos(
        subscription,
        node,
        type_info,
        topic_name,
        callback,
        context,
        &crate::qos::NANO_ROS_QOS_SENSOR_DATA,
    )
}

/// Initialize a subscription with custom QoS.
///
/// # Safety
/// See `nano_ros_subscription_init` for safety requirements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_init_with_qos(
    subscription: *mut nano_ros_subscription_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
    callback: nano_ros_subscription_callback_t,
    context: *mut c_void,
    qos: *const nano_ros_qos_t,
) -> nano_ros_ret_t {
    // Validate required arguments
    if subscription.is_null() || node.is_null() || type_info.is_null() || topic_name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    if callback.is_none() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let subscription = &mut *subscription;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if subscription is already initialized
    if subscription.state
        != nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_UNINITIALIZED
    {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if node is initialized
    if node_ref.state != nano_ros_node_state_t::NANO_ROS_NODE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Copy topic name
    let topic_ptr = topic_name as *const u8;
    let mut len = 0usize;
    while len < MAX_TOPIC_LEN - 1 {
        let c = *topic_ptr.add(len);
        if c == 0 {
            break;
        }
        subscription.topic_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }
    subscription.topic_name[len] = 0;
    subscription.topic_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            subscription.type_name[len] = c;
            len += 1;
        }
        subscription.type_name[len] = 0;
        subscription.type_name_len = len;
    }

    // Copy type hash
    if !type_info.type_hash.is_null() {
        let hash_ptr = type_info.type_hash as *const u8;
        len = 0;
        while len < MAX_TYPE_HASH_LEN - 1 {
            let c = *hash_ptr.add(len);
            if c == 0 {
                break;
            }
            subscription.type_hash[len] = c;
            len += 1;
        }
        subscription.type_hash[len] = 0;
        subscription.type_hash_len = len;
    }

    // Store callback and context
    subscription.callback = callback;
    subscription.context = context;
    subscription.node = node;

    // Get QoS settings
    let _qos_settings = if qos.is_null() {
        crate::qos::NANO_ROS_QOS_DEFAULT.to_qos_settings()
    } else {
        (*qos).to_qos_settings()
    };

    // Create the internal subscriber using zenoh
    #[cfg(feature = "std")]
    {
        use nano_ros_transport::{Session, TopicInfo, ZenohSession};

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        if support_mut.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
            return NANO_ROS_RET_NOT_INIT;
        }

        // Save domain_id before borrowing session
        let domain_id = support_mut.domain_id as u32;

        // Get mutable session reference
        let session: &mut ZenohSession = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        // Build the topic key expression for ROS 2 compatibility
        let topic_str =
            core::str::from_utf8_unchecked(&subscription.topic_name[..subscription.topic_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&subscription.type_name[..subscription.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&subscription.type_hash[..subscription.type_hash_len]);

        // Build TopicInfo
        let topic_info = TopicInfo::new(topic_str, type_str, type_hash_str).with_domain(domain_id);

        // Create subscriber (uses polling model - executor will poll and invoke callbacks)
        match session.create_subscriber(&topic_info, _qos_settings) {
            Ok(sub_handle) => {
                let sub_box = std::boxed::Box::new(sub_handle);
                subscription._internal = std::boxed::Box::into_raw(sub_box) as *mut _;
            }
            Err(_) => return NANO_ROS_RET_ERROR,
        }

        subscription.state = nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED;
        NANO_ROS_RET_OK
    }

    #[cfg(not(feature = "std"))]
    {
        // For no_std, use shim transport (not yet implemented)
        NANO_ROS_RET_ERROR
    }
}

/// Finalize a subscription.
///
/// # Parameters
/// * `subscription` - Pointer to an initialized subscription
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if subscription is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `subscription` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_fini(
    subscription: *mut nano_ros_subscription_t,
) -> nano_ros_ret_t {
    if subscription.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let subscription = &mut *subscription;

    if subscription.state != nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clean up internal resources
    #[cfg(feature = "std")]
    {
        if !subscription._internal.is_null() {
            use nano_ros_transport::ZenohSubscriber;
            let _sub = std::boxed::Box::from_raw(subscription._internal as *mut ZenohSubscriber);
            // Subscriber is dropped here
        }
    }

    subscription._internal = ptr::null_mut();
    subscription.callback = None;
    subscription.context = ptr::null_mut();
    subscription.node = ptr::null();
    subscription.state = nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Get the topic name of a subscription.
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_get_topic_name(
    subscription: *const nano_ros_subscription_t,
) -> *const c_char {
    if subscription.is_null() {
        return ptr::null();
    }

    let subscription = &*subscription;
    if subscription.state != nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED
    {
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
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_subscription_is_valid(
    subscription: *const nano_ros_subscription_t,
) -> c_int {
    if subscription.is_null() {
        return 0;
    }

    let subscription = &*subscription;
    if subscription.state == nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED
    {
        1
    } else {
        0
    }
}

// Internal helper methods for executor
impl nano_ros_subscription_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> nano_ros_subscription_callback_t {
        self.callback
    }

    /// Get the user context
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Get the internal subscriber handle
    pub(crate) fn get_internal(&self) -> *mut c_void {
        self._internal
    }
}
