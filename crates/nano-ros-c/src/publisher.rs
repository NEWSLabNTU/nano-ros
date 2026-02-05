//! Publisher API for nano-ros C API.
//!
//! Publishers send messages to topics that subscribers can receive.

use core::ffi::{c_char, c_int};
use core::ptr;

use crate::constants::{MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nano_ros_node_state_t, nano_ros_node_t};
use crate::qos::nano_ros_qos_t;
use crate::support::nano_ros_support_state_t;

/// Message type information.
///
/// This structure describes a ROS message type for use with publishers
/// and subscribers.
#[repr(C)]
pub struct nano_ros_message_type_t {
    /// Type name (e.g., "std_msgs::msg::dds_::Int32")
    pub type_name: *const c_char,
    /// Type hash (RIHS format)
    pub type_hash: *const c_char,
    /// Maximum serialized size (0 = dynamic/unknown)
    pub serialized_size_max: usize,
}

/// Publisher state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_publisher_state_t {
    /// Not initialized
    NANO_ROS_PUBLISHER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_PUBLISHER_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_PUBLISHER_STATE_SHUTDOWN = 2,
}

/// Publisher structure.
#[repr(C)]
pub struct nano_ros_publisher_t {
    /// Current state
    pub state: nano_ros_publisher_state_t,
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
    /// Pointer to parent node
    node: *const nano_ros_node_t,
    /// Opaque pointer to internal Rust publisher
    _internal: *mut core::ffi::c_void,
}

impl Default for nano_ros_publisher_t {
    fn default() -> Self {
        Self {
            state: nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

/// Get a zero-initialized publisher.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_publisher_get_zero_initialized() -> nano_ros_publisher_t {
    nano_ros_publisher_t::default()
}

/// Initialize a publisher with default QoS (RELIABLE, KEEP_LAST(10)).
///
/// This is the recommended initialization function for most use cases.
/// Uses `QOS_PROFILE_DEFAULT` which provides reliable delivery.
///
/// # Parameters
/// * `publisher` - Pointer to a zero-initialized publisher
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_init(
    publisher: *mut nano_ros_publisher_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
) -> nano_ros_ret_t {
    nano_ros_publisher_init_with_qos(publisher, node, type_info, topic_name, ptr::null())
}

/// Initialize a publisher with default QoS (RELIABLE, KEEP_LAST(10)).
///
/// Alias for `nano_ros_publisher_init()` for rclc API compatibility.
///
/// # Safety
/// See `nano_ros_publisher_init()` for safety requirements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_init_default(
    publisher: *mut nano_ros_publisher_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
) -> nano_ros_ret_t {
    nano_ros_publisher_init_with_qos(publisher, node, type_info, topic_name, ptr::null())
}

/// Initialize a publisher with best-effort QoS (BEST_EFFORT, VOLATILE).
///
/// Use this for sensor data or high-frequency topics where occasional
/// message loss is acceptable but low latency is preferred.
///
/// # Parameters
/// * `publisher` - Pointer to a zero-initialized publisher
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_init_best_effort(
    publisher: *mut nano_ros_publisher_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
) -> nano_ros_ret_t {
    nano_ros_publisher_init_with_qos(
        publisher,
        node,
        type_info,
        topic_name,
        &crate::qos::NANO_ROS_QOS_SENSOR_DATA,
    )
}

/// Initialize a publisher with custom QoS.
///
/// # Parameters
/// * `publisher` - Pointer to a zero-initialized publisher
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
/// * `qos` - Pointer to QoS settings (NULL for default)
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_init_with_qos(
    publisher: *mut nano_ros_publisher_t,
    node: *const nano_ros_node_t,
    type_info: *const nano_ros_message_type_t,
    topic_name: *const c_char,
    qos: *const nano_ros_qos_t,
) -> nano_ros_ret_t {
    // Validate required arguments
    if publisher.is_null() || node.is_null() || type_info.is_null() || topic_name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let publisher = &mut *publisher;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if publisher is already initialized
    if publisher.state != nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_UNINITIALIZED {
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
        publisher.topic_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }
    publisher.topic_name[len] = 0;
    publisher.topic_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            publisher.type_name[len] = c;
            len += 1;
        }
        publisher.type_name[len] = 0;
        publisher.type_name_len = len;
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
            publisher.type_hash[len] = c;
            len += 1;
        }
        publisher.type_hash[len] = 0;
        publisher.type_hash_len = len;
    }

    // Store node reference
    publisher.node = node;

    // Get QoS settings
    let _qos_settings = if qos.is_null() {
        crate::qos::NANO_ROS_QOS_DEFAULT.to_qos_settings()
    } else {
        (*qos).to_qos_settings()
    };

    // Create the internal publisher using zenoh
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
            core::str::from_utf8_unchecked(&publisher.topic_name[..publisher.topic_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&publisher.type_name[..publisher.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&publisher.type_hash[..publisher.type_hash_len]);

        // Build TopicInfo
        let topic_info = TopicInfo::new(topic_str, type_str, type_hash_str).with_domain(domain_id);

        // Create publisher
        match session.create_publisher(&topic_info, _qos_settings) {
            Ok(pub_handle) => {
                let pub_box = std::boxed::Box::new(pub_handle);
                publisher._internal = std::boxed::Box::into_raw(pub_box) as *mut _;
            }
            Err(_) => return NANO_ROS_RET_ERROR,
        }

        publisher.state = nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_INITIALIZED;
        NANO_ROS_RET_OK
    }

    #[cfg(not(feature = "std"))]
    {
        // For no_std, use shim transport (not yet implemented)
        NANO_ROS_RET_ERROR
    }
}

/// Publish raw CDR-serialized data.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
/// * `data` - Pointer to CDR-serialized message data
/// * `len` - Length of data in bytes
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL or len is 0
/// * `NANO_ROS_RET_NOT_INIT` if publisher is not initialized
/// * `NANO_ROS_RET_PUBLISH_FAILED` on publish failure
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher
/// * `data` must be a valid pointer to `len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publish_raw(
    publisher: *const nano_ros_publisher_t,
    data: *const u8,
    len: usize,
) -> nano_ros_ret_t {
    if publisher.is_null() || data.is_null() || len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let publisher = &*publisher;

    if publisher.state != nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    #[cfg(feature = "std")]
    {
        use nano_ros_transport::{Publisher, ZenohPublisher};

        if publisher._internal.is_null() {
            return NANO_ROS_RET_NOT_INIT;
        }

        let pub_handle = &*(publisher._internal as *const ZenohPublisher);
        let data_slice = core::slice::from_raw_parts(data, len);

        match pub_handle.publish_raw(data_slice) {
            Ok(()) => NANO_ROS_RET_OK,
            Err(_) => NANO_ROS_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "std"))]
    {
        NANO_ROS_RET_ERROR
    }
}

/// Finalize a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if publisher is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `publisher` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_fini(
    publisher: *mut nano_ros_publisher_t,
) -> nano_ros_ret_t {
    if publisher.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let publisher = &mut *publisher;

    if publisher.state != nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clean up internal resources
    #[cfg(feature = "std")]
    {
        if !publisher._internal.is_null() {
            use nano_ros_transport::ZenohPublisher;
            let _pub = std::boxed::Box::from_raw(publisher._internal as *mut ZenohPublisher);
            // Publisher is dropped here
        }
    }

    publisher._internal = ptr::null_mut();
    publisher.node = ptr::null();
    publisher.state = nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Get the topic name of a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_get_topic_name(
    publisher: *const nano_ros_publisher_t,
) -> *const c_char {
    if publisher.is_null() {
        return ptr::null();
    }

    let publisher = &*publisher;
    if publisher.state != nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_INITIALIZED {
        return ptr::null();
    }

    publisher.topic_name.as_ptr() as *const c_char
}

/// Check if publisher is valid (initialized).
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_publisher_is_valid(
    publisher: *const nano_ros_publisher_t,
) -> c_int {
    if publisher.is_null() {
        return 0;
    }

    let publisher = &*publisher;
    if publisher.state == nano_ros_publisher_state_t::NANO_ROS_PUBLISHER_STATE_INITIALIZED {
        1
    } else {
        0
    }
}
