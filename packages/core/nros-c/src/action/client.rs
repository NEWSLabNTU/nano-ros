//! Action client implementation.

use core::ffi::c_void;
use core::ptr;

use super::common::*;
use crate::constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};

// ============================================================================
// Internal implementation
// ============================================================================

/// Internal state for the action client.
///
/// Holds the `ActionClientCore` created during `nros_action_client_init()`.
/// The core contains 3 service clients (send_goal, cancel_goal, get_result)
/// and 1 feedback subscriber.
#[cfg(feature = "alloc")]
struct ActionClientInternal {
    core: nros_node::ActionClientCore<
        { crate::executor::MESSAGE_BUFFER_SIZE },
        { crate::executor::MESSAGE_BUFFER_SIZE },
        { crate::executor::MESSAGE_BUFFER_SIZE },
    >,
}

// ============================================================================
// Action Client
// ============================================================================

/// Action client state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_action_client_state_t {
    /// Not initialized
    NROS_ACTION_CLIENT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_ACTION_CLIENT_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_ACTION_CLIENT_STATE_SHUTDOWN = 2,
}

/// Action client structure.
#[repr(C)]
pub struct nros_action_client_t {
    /// Current state
    pub state: nros_action_client_state_t,
    /// Action name storage
    pub action_name: [u8; MAX_ACTION_NAME_LEN],
    /// Action name length
    pub action_name_len: usize,
    /// Type name storage
    pub type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    pub type_name_len: usize,
    /// Type hash storage
    pub type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    pub type_hash_len: usize,
    /// Feedback callback
    pub feedback_callback: nros_feedback_callback_t,
    /// Result callback
    pub result_callback: nros_result_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Opaque pointer to internal implementation
    pub _internal: *mut c_void,
}

impl Default for nros_action_client_t {
    fn default() -> Self {
        Self {
            state: nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
            action_name: [0u8; MAX_ACTION_NAME_LEN],
            action_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            feedback_callback: None,
            result_callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

// ============================================================================
// Action Client Functions
// ============================================================================

/// Get a zero-initialized action client.
#[unsafe(no_mangle)]
pub extern "C" fn nros_action_client_get_zero_initialized() -> nros_action_client_t {
    nros_action_client_t::default()
}

/// Initialize an action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_init(
    client: *mut nros_action_client_t,
    node: *const nros_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const nros_action_type_t,
) -> nros_ret_t {
    validate_not_null!(client, node, action_name, type_info);

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy action name
    let name_ptr = action_name as *const u8;
    let mut len = 0usize;
    while len < MAX_ACTION_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        client.action_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    client.action_name[len] = 0;
    client.action_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            client.type_name[len] = c;
            len += 1;
        }
        client.type_name[len] = 0;
        client.type_name_len = len;
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
            client.type_hash[len] = c;
            len += 1;
        }
        client.type_hash[len] = 0;
        client.type_hash_len = len;
    }

    // Store node pointer
    client.node = node;

    // Create the ActionClientCore with 3 service clients + 1 feedback subscriber.
    // This follows the service client init pattern (service.rs:590-634).
    #[cfg(feature = "alloc")]
    {
        use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session, TopicInfo};

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        if support_mut.state != crate::support::nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
        {
            return NROS_RET_NOT_INIT;
        }

        let domain_id = support_mut.domain_id as u32;

        let session = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        let action_name_str =
            core::str::from_utf8_unchecked(&client.action_name[..client.action_name_len]);
        let type_str = core::str::from_utf8_unchecked(&client.type_name[..client.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client.type_hash[..client.type_hash_len]);

        let action_info =
            ActionInfo::new(action_name_str, type_str, type_hash_str).with_domain(domain_id);

        // Create send_goal service client
        let send_goal_keyexpr: nros_core::heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, type_str, type_hash_str).with_domain(0);
        let send_goal_client = match session.create_service_client(&send_goal_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create cancel_goal service client
        let cancel_goal_keyexpr: nros_core::heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            type_hash_str,
        )
        .with_domain(0);
        let cancel_goal_client = match session.create_service_client(&cancel_goal_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create get_result service client
        let get_result_keyexpr: nros_core::heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, type_str, type_hash_str).with_domain(0);
        let get_result_client = match session.create_service_client(&get_result_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create feedback subscriber (best-effort QoS)
        let feedback_keyexpr: nros_core::heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, type_str, type_hash_str).with_domain(0);
        let feedback_subscriber =
            match session.create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT) {
                Ok(s) => s,
                Err(_) => return NROS_RET_ERROR,
            };

        // Construct ActionClientCore
        let core = nros_node::ActionClientCore::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
        );

        let internal = alloc::boxed::Box::new(ActionClientInternal { core });
        client._internal = alloc::boxed::Box::into_raw(internal) as *mut _;
    }

    client.state = nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set feedback callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_set_feedback_callback(
    client: *mut nros_action_client_t,
    callback: nros_feedback_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.feedback_callback = callback;
    client.context = context;

    NROS_RET_OK
}

/// Set result callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_set_result_callback(
    client: *mut nros_action_client_t,
    callback: nros_result_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.result_callback = callback;
    client.context = context;

    NROS_RET_OK
}

/// Send a goal request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_send_goal(
    client: *mut nros_action_client_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let goal_data = core::slice::from_raw_parts(goal, goal_len);

        match internal.core.send_goal_raw(goal_data) {
            Ok(goal_id) => {
                // Copy the generated GoalId to the output UUID
                let uuid = &mut *goal_uuid;
                uuid.uuid = goal_id.uuid;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Request cancellation of a goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_cancel_goal(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let uuid = &*goal_uuid;
        let goal_id = nros_core::GoalId { uuid: uuid.uuid };

        match internal.core.send_cancel_request(&goal_id) {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Request result of a goal (blocking).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_get_result(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
    status: *mut nros_goal_status_t,
    result: *mut u8,
    result_capacity: usize,
    result_len: *mut usize,
) -> nros_ret_t {
    validate_not_null!(client, goal_uuid, status, result, result_len);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let uuid = &*goal_uuid;
        let goal_id = nros_core::GoalId { uuid: uuid.uuid };

        // Send the get_result request
        if internal.core.send_get_result_request(&goal_id).is_err() {
            return NROS_RET_ERROR;
        }

        // Poll for the reply with timeout (same approach as nros_client_call)
        let mut attempts = 0u32;
        loop {
            match internal.core.try_recv_get_result_reply() {
                Ok(Some(len)) => {
                    // GetResult response CDR: header (4) + status (int8=1 + 3 pad) + result
                    if len < 4 {
                        return NROS_RET_ERROR;
                    }

                    // Status is the first byte after CDR header
                    let buf = internal.core.result_buffer_ref();
                    let raw_status = buf[4];
                    *status = match raw_status {
                        1 => nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
                        2 => nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
                        3 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
                        4 => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
                        5 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
                        6 => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
                        _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
                    };

                    // Result data starts after CDR header (4) + status (1) + padding (3)
                    let result_offset = 8usize;
                    let result_data_len = len.saturating_sub(result_offset);

                    if result_data_len > result_capacity {
                        return NROS_RET_ERROR;
                    }

                    let out = core::slice::from_raw_parts_mut(result, result_capacity);
                    out[..result_data_len]
                        .copy_from_slice(&buf[result_offset..result_offset + result_data_len]);
                    *result_len = result_data_len;

                    return NROS_RET_OK;
                }
                Ok(None) => {
                    attempts += 1;
                    if attempts > 3000 {
                        return NROS_RET_TIMEOUT;
                    }
                    #[cfg(feature = "std")]
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    #[cfg(not(feature = "std"))]
                    core::hint::spin_loop();
                }
                Err(_) => return NROS_RET_ERROR,
            }
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Try to receive feedback for an active goal (non-blocking).
///
/// If feedback is available, invokes the feedback callback (if set).
/// Returns `NROS_RET_OK` if feedback was received and dispatched,
/// `NROS_RET_TIMEOUT` if no feedback is available.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_try_recv_feedback(
    client: *mut nros_action_client_t,
) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);

        match internal.core.try_recv_feedback_raw() {
            Ok(Some((goal_id, len))) => {
                // Feedback CDR contains: CDR header (4) + GoalId (16) + feedback data
                // The GoalId has already been parsed by try_recv_feedback_raw.
                // The full raw data (including CDR header + GoalId) is in feedback_buffer.
                // We need to extract just the feedback payload for the C callback.
                // After CDR header (4) + GoalId (16 bytes via CDR) = ~24 bytes offset
                // However, the exact offset depends on CDR encoding. The feedback_buffer
                // contains the full received message. For the C callback, pass the raw
                // feedback bytes starting after the GoalId framing.

                if let Some(cb) = client.feedback_callback {
                    let uuid = nros_goal_uuid_t { uuid: goal_id.uuid };

                    // Feedback CDR layout: header (4) + GoalId UUID (16) = 20 bytes
                    let fb_offset = 20usize;
                    let fb_len = len.saturating_sub(fb_offset);
                    let fb_ptr = if fb_len > 0 {
                        internal.core.feedback_buffer_ref()[fb_offset..].as_ptr()
                    } else {
                        ptr::null()
                    };

                    cb(&uuid, fb_ptr, fb_len, client.context);
                }

                NROS_RET_OK
            }
            Ok(None) => NROS_RET_TIMEOUT,
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Finalize an action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_fini(client: *mut nros_action_client_t) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    // Drop the internal ActionClientCore
    #[cfg(feature = "alloc")]
    {
        if !client._internal.is_null() {
            let _internal =
                alloc::boxed::Box::from_raw(client._internal as *mut ActionClientInternal);
            // ActionClientInternal (and its ActionClientCore) is dropped here
        }
    }

    client._internal = ptr::null_mut();
    client.feedback_callback = None;
    client.result_callback = None;
    client.context = ptr::null_mut();
    client.node = ptr::null();
    client.state = nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;
    use core::ptr;

    // Helper to create a dummy action type info
    fn dummy_action_type() -> nros_action_type_t {
        let type_name = b"example_interfaces::action::dds_::Fibonacci_\0";
        let type_hash = b"RIHS01_test\0";
        nros_action_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
            goal_serialized_size_max: 8,
            result_serialized_size_max: 264,
            feedback_serialized_size_max: 264,
        }
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_null_ptrs() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        // NULL client
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    ptr::null_mut(),
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    ptr::null(),
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL action_name
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_client_init(&mut cli, &node, ptr::null(), &type_info) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_uninit_node() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_zero_initialized_state() {
        let cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            cli.state,
            nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
        );
        assert!(cli.node.is_null());
        assert!(cli._internal.is_null());
        assert!(cli.feedback_callback.is_none());
        assert!(cli.result_callback.is_none());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe { nros_action_client_fini(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_client_fini(&mut cli) },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_send_goal_null_ptrs() {
        let goal_data = [0u8; 8];
        let mut uuid = nros_goal_uuid_t::default();

        // NULL client
        assert_eq!(
            unsafe { nros_action_send_goal(ptr::null_mut(), goal_data.as_ptr(), 8, &mut uuid) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL goal
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_send_goal(&mut cli, ptr::null(), 0, &mut uuid) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        assert_eq!(
            unsafe { nros_action_send_goal(&mut cli, goal_data.as_ptr(), 8, ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_get_result_null_ptrs() {
        let uuid = nros_goal_uuid_t::default();
        let mut status = nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN;
        let mut result_buf = [0u8; 8];
        let mut result_len: usize = 0;

        // NULL client
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    ptr::null_mut(),
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    ptr::null(),
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL status
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    ptr::null_mut(),
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL result
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    ptr::null_mut(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL result_len
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }
}
