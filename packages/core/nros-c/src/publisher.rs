//! Publisher API for nros C API.
//!
//! Publishers send messages to topics that subscribers can receive.

use core::ffi::c_char;
use core::ptr;

use crate::constants::{
    MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN, PUBLISHER_OPAQUE_U64S,
};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};
use crate::qos::nros_qos_t;
use crate::support::nros_support_state_t;

/// Message type information.
///
/// This structure describes a ROS message type for use with publishers
/// and subscribers.
#[repr(C)]
pub struct nros_message_type_t {
    /// Type name (e.g., "std_msgs::msg::dds_::Int32")
    pub type_name: *const c_char,
    /// Type hash (RIHS format)
    pub type_hash: *const c_char,
    /// Maximum serialized size (0 = dynamic/unknown)
    pub serialized_size_max: usize,
}

/// Service type information.
///
/// Provides type name and hash for a ROS 2 service type.
/// Used by generated code from `nano_ros_generate_interfaces()`.
#[repr(C)]
pub struct nros_service_type_t {
    /// Type name (e.g., "example_interfaces::srv::dds_::AddTwoInts_")
    pub type_name: *const c_char,
    /// Type hash (RIHS format)
    pub type_hash: *const c_char,
}

/// Publisher state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_publisher_state_t {
    /// Not initialized
    NROS_PUBLISHER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_PUBLISHER_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_PUBLISHER_STATE_SHUTDOWN = 2,
}

/// Publisher structure.
#[repr(C)]
pub struct nros_publisher_t {
    /// Current state
    pub state: nros_publisher_state_t,
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
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Inline opaque storage for the RMW publisher handle.
    /// Avoids heap allocation — managed by nros_publisher_init/fini.
    pub _opaque: [u64; PUBLISHER_OPAQUE_U64S],
}

impl Default for nros_publisher_t {
    fn default() -> Self {
        Self {
            state: nros_publisher_state_t::NROS_PUBLISHER_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            node: ptr::null(),
            _opaque: [0u64; PUBLISHER_OPAQUE_U64S],
        }
    }
}

// PUBLISHER_OPAQUE_U64S is computed from size_of::<RmwPublisher>() in opaque_sizes.rs —
// always large enough by construction.

/// Get a zero-initialized publisher.
#[unsafe(no_mangle)]
pub extern "C" fn nros_publisher_get_zero_initialized() -> nros_publisher_t {
    nros_publisher_t::default()
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NROS_RET_NOT_INIT` if node is not initialized
/// * `NROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_init(
    publisher: *mut nros_publisher_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
) -> nros_ret_t {
    nros_publisher_init_with_qos(publisher, node, type_info, topic_name, ptr::null())
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NROS_RET_NOT_INIT` if node is not initialized
/// * `NROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_init_with_qos(
    publisher: *mut nros_publisher_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
    qos: *const nros_qos_t,
) -> nros_ret_t {
    validate_not_null!(publisher, node, type_info, topic_name);

    let publisher = &mut *publisher;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy topic name (required — empty rejected)
    publisher.topic_name_len = crate::util::copy_cstr_into(topic_name, &mut publisher.topic_name);
    if publisher.topic_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    publisher.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut publisher.type_name);
    publisher.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut publisher.type_hash);

    // Store node reference
    publisher.node = node;

    // Get QoS settings
    let _qos_settings = if qos.is_null() {
        crate::qos::NROS_QOS_DEFAULT.to_qos_settings()
    } else {
        (*qos).to_qos_settings()
    };

    // Create the internal publisher
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        use nros_node::{Session, TopicInfo};

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        validate_state!(
            support_mut,
            nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
        );

        // Save domain_id before borrowing session
        let domain_id = support_mut.domain_id as u32;

        // Get mutable session reference
        let session = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        // Build the topic key expression for ROS 2 compatibility
        let topic_str =
            core::str::from_utf8_unchecked(&publisher.topic_name[..publisher.topic_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&publisher.type_name[..publisher.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&publisher.type_hash[..publisher.type_hash_len]);

        // Pull node identity for liveliness — without these, no liveliness token
        // is declared and rmw_zenoh-style routing won't deliver messages.
        let node_name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
        let namespace_str =
            core::str::from_utf8_unchecked(&node_ref.namespace[..node_ref.namespace_len]);

        // Build TopicInfo
        let topic_info = TopicInfo::new(topic_str, type_str, type_hash_str)
            .with_domain(domain_id)
            .with_node_name(node_name_str)
            .with_namespace(namespace_str);

        // Create publisher — write handle directly into inline opaque storage
        match session.create_publisher(&topic_info, _qos_settings) {
            Ok(pub_handle) => {
                core::ptr::write(
                    publisher._opaque.as_mut_ptr() as *mut nros::internals::RmwPublisher,
                    pub_handle,
                );
            }
            Err(_) => return NROS_RET_ERROR,
        }

        publisher.state = nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED;
        NROS_RET_OK
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
    {
        NROS_RET_ERROR
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or len is 0
/// * `NROS_RET_NOT_INIT` if publisher is not initialized
/// * `NROS_RET_PUBLISH_FAILED` on publish failure
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher
/// * `data` must be a valid pointer to `len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publish_raw(
    publisher: *const nros_publisher_t,
    data: *const u8,
    len: usize,
) -> nros_ret_t {
    validate_not_null!(publisher, data);
    if len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let publisher = &*publisher;

    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        let data_slice = core::slice::from_raw_parts(data, len);

        match pub_handle.publish_raw(data_slice) {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
    {
        NROS_RET_ERROR
    }
}

/// Finalize a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if publisher is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `publisher` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_fini(publisher: *mut nros_publisher_t) -> nros_ret_t {
    validate_not_null!(publisher);

    let publisher = &mut *publisher;

    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );

    // Drop the inline RMW publisher handle
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        core::ptr::drop_in_place(
            publisher._opaque.as_mut_ptr() as *mut nros::internals::RmwPublisher
        );
    }

    publisher._opaque = [0u64; PUBLISHER_OPAQUE_U64S];
    publisher.node = ptr::null();
    publisher.state = nros_publisher_state_t::NROS_PUBLISHER_STATE_SHUTDOWN;

    NROS_RET_OK
}

/// Get the topic name of a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_get_topic_name(
    publisher: *const nros_publisher_t,
) -> *const c_char {
    if publisher.is_null() {
        return ptr::null();
    }

    let publisher = &*publisher;
    if publisher.state != nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED {
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
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_is_valid(publisher: *const nros_publisher_t) -> bool {
    if publisher.is_null() {
        return false;
    }

    let publisher = &*publisher;
    publisher.state == nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
}

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn publisher_init_null_ptrs() {
        let topic = b"/chatter\0";
        let type_name = b"std_msgs::msg::dds_::Int32\0";
        let type_hash = b"RIHS01_test\0";
        let type_info = nros_message_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
            serialized_size_max: 4,
        };

        let mut node = crate::node::nros_node_get_zero_initialized();

        // NULL publisher → INVALID_ARGUMENT
        assert_eq!(
            unsafe {
                nros_publisher_init(
                    core::ptr::null_mut(),
                    &node,
                    &type_info,
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node → INVALID_ARGUMENT
        let mut pub_ = nros_publisher_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_publisher_init(
                    &mut pub_,
                    core::ptr::null(),
                    &type_info,
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info → INVALID_ARGUMENT
        let mut pub_ = nros_publisher_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_publisher_init(
                    &mut pub_,
                    &node,
                    core::ptr::null(),
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL topic → INVALID_ARGUMENT
        let mut pub_ = nros_publisher_get_zero_initialized();
        assert_eq!(
            unsafe { nros_publisher_init(&mut pub_, &node, &type_info, core::ptr::null()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn publisher_zero_initialized_state() {
        let pub_ = nros_publisher_get_zero_initialized();
        assert_eq!(
            pub_.state,
            nros_publisher_state_t::NROS_PUBLISHER_STATE_UNINITIALIZED,
        );
        assert!(pub_.node.is_null());
        assert!(pub_._opaque.iter().all(|&v| v == 0));
    }
}
