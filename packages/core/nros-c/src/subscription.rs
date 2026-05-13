//! Subscription API for nros C API.
//!
//! Subscriptions receive messages from topics that publishers send to.

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use crate::{
    constants::{MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN},
    error::*,
    node::{nros_node_state_t, nros_node_t},
    opaque_sizes::SUBSCRIPTION_OPAQUE_U64S,
    publisher::nros_message_type_t,
    qos::nros_qos_t,
    support::nros_support_state_t,
};

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
    /// Initialized for callback dispatch (Layer 2). Subscriber entity
    /// creation is deferred to `nros_executor_register_subscription`.
    NROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
    /// Phase 122.3.b — initialized for primitive-mode polling (Layer 1).
    /// Subscriber entity created at init time and stored inline in
    /// `_opaque`; caller drains via `nros_subscription_try_recv_raw`.
    /// No executor registration.
    NROS_SUBSCRIPTION_STATE_POLLING = 3,
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
    /// Phase 122.3.b — inline opaque storage for the L1 polling-mode
    /// `RawSubscription<MESSAGE_BUFFER_SIZE>`. Zeroed in callback (L2)
    /// mode; populated by `nros_subscription_init_polling`.
    pub _opaque: [u64; SUBSCRIPTION_OPAQUE_U64S],
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
            _opaque: [0u64; SUBSCRIPTION_OPAQUE_U64S],
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

    // Subscriber creation is deferred to nros_executor_register_subscription(),
    // which calls nros_node::Executor::register_subscription_raw_with_qos_sized().
    subscription.handle_id = usize::MAX;
    subscription.state = nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED;

    NROS_RET_OK
}

// ============================================================================
// Phase 122.3.b — Layer-1 primitive entry points (caller polls)
// ============================================================================
//
// L1 is for callers that own their own scheduler (RTIC, embassy,
// FreeRTOS-native task-per-entity). The subscriber entity is created
// at init time and stored inline in `_opaque`; the caller polls it
// directly via `nros_subscription_try_recv_raw`, never registering
// with an `nros_executor_t`.
//
// L1 ops are mutually exclusive with L2 (callback / executor). A
// subscription in `POLLING` state cannot be registered with an
// executor; one in `INITIALIZED` (L2) state cannot be polled
// directly. State transitions:
//
//   UNINITIALIZED → POLLING (via nros_subscription_init_polling)
//                 → INITIALIZED (via nros_subscription_init,
//                                deferred subscriber creation)
//   POLLING / INITIALIZED → SHUTDOWN (via nros_subscription_fini)

/// Phase 122.3.b — initialize an L1 polling-mode subscription.
///
/// Creates the underlying RMW subscriber immediately and stores it
/// inline in the subscription's `_opaque` field. The caller drains
/// received messages via `nros_subscription_try_recv_raw`.
///
/// Uses default QoS (RELIABLE, KEEP_LAST(10)). For custom QoS, use
/// `nros_subscription_init_polling_with_qos`.
///
/// # Parameters
/// * `subscription` - Pointer to a zero-initialized subscription
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or topic empty
/// * `NROS_RET_NOT_INIT` if node / support not initialized
/// * `NROS_RET_ERROR` if subscriber creation failed
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_init_polling(
    subscription: *mut nros_subscription_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
) -> nros_ret_t {
    nros_subscription_init_polling_with_qos(subscription, node, type_info, topic_name, ptr::null())
}

/// Phase 122.3.b — initialize an L1 polling-mode subscription with custom QoS.
///
/// See `nros_subscription_init_polling` for the threading + lifecycle
/// contract.
///
/// # Safety
/// See `nros_subscription_init_polling`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_init_polling_with_qos(
    subscription: *mut nros_subscription_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
    qos: *const nros_qos_t,
) -> nros_ret_t {
    validate_not_null!(subscription, node, type_info, topic_name);

    let subscription_mut = &mut *subscription;
    let node_ref = &*node;
    let type_info_ref = &*type_info;

    validate_state!(
        subscription_mut,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy topic + type metadata into the subscription struct so
    // getters keep working post-init.
    subscription_mut.topic_name_len =
        crate::util::copy_cstr_into(topic_name, &mut subscription_mut.topic_name);
    if subscription_mut.topic_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    subscription_mut.type_name_len =
        crate::util::copy_cstr_into(type_info_ref.type_name, &mut subscription_mut.type_name);
    subscription_mut.type_hash_len =
        crate::util::copy_cstr_into(type_info_ref.type_hash, &mut subscription_mut.type_hash);

    subscription_mut.node = node;
    subscription_mut.qos = if qos.is_null() {
        crate::qos::NROS_QOS_DEFAULT
    } else {
        *qos
    };
    subscription_mut.callback = None;
    subscription_mut.context = ptr::null_mut();
    subscription_mut.handle_id = usize::MAX;

    // Create the subscriber NOW (vs deferred for L2). The L1 path
    // owns the entity inline; no executor arena involved.
    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::{Session, TopicInfo};

        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };
        validate_state!(
            support_mut,
            nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
        );
        let domain_id = support_mut.domain_id as u32;
        let session = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        let topic_str = core::str::from_utf8_unchecked(
            &subscription_mut.topic_name[..subscription_mut.topic_name_len],
        );
        let type_str = core::str::from_utf8_unchecked(
            &subscription_mut.type_name[..subscription_mut.type_name_len],
        );
        let type_hash_str = core::str::from_utf8_unchecked(
            &subscription_mut.type_hash[..subscription_mut.type_hash_len],
        );
        let node_name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
        let namespace_str =
            core::str::from_utf8_unchecked(&node_ref.namespace[..node_ref.namespace_len]);

        let topic_info = TopicInfo::new(topic_str, type_str, type_hash_str)
            .with_domain(domain_id)
            .with_node_name(node_name_str)
            .with_namespace(namespace_str);

        let qos_settings = subscription_mut.qos.to_qos_settings();
        match session.create_subscriber(&topic_info, qos_settings) {
            Ok(handle) => {
                let raw = nros_node::RawSubscription::<{ crate::config::MESSAGE_BUFFER_SIZE }>::new(
                    handle,
                );
                core::ptr::write(
                    subscription_mut._opaque.as_mut_ptr()
                        as *mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>,
                    raw,
                );
            }
            Err(_) => return NROS_RET_ERROR,
        }
    }

    subscription_mut.state = nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_POLLING;
    NROS_RET_OK
}

/// Phase 122.3.c.6.e — register a C wake callback on an L1
/// polling-mode subscription. `state` is a caller-owned
/// `nros_wake_state_t` (declared next to the subscription) that
/// must outlive the subscription and not move. Pass `cb = NULL`
/// to disable. The backend wakes the callback when a new message
/// arrives.
///
/// # Safety
/// All pointers valid; `state` storage stable for the
/// subscription's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_set_wake_callback(
    subscription: *mut nros_subscription_t,
    state: *mut crate::service::nros_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_ret_t {
    if subscription.is_null() || state.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_POLLING {
        return NROS_RET_INVALID_ARGUMENT;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let state_ptr = state as *mut nros_node::c_waker::CWakeState;
        core::ptr::write(
            state_ptr,
            nros_node::c_waker::CWakeState { fn_ptr: cb, ctx },
        );
        let waker = nros_node::c_waker::make_waker(state_ptr);
        let raw = &*(subscription_mut._opaque.as_ptr()
            as *const nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>);
        raw.register_waker(&waker);
        NROS_RET_OK
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (state, cb, ctx);
        NROS_RET_NOT_INIT
    }
}

/// Phase 122.3.b — non-blocking poll on an L1 polling-mode
/// subscription. Returns the number of bytes received on success
/// (may be 0 if no data available), or a negative `nros_ret_t` on
/// error.
///
/// # Parameters
/// * `subscription` - Pointer to a POLLING-state subscription
/// * `buf` - Caller-supplied buffer to receive the message bytes
/// * `buf_len` - Capacity of `buf`
///
/// # Returns
/// * `>= 0` — number of bytes copied into `buf`
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or state is wrong
/// * `NROS_RET_ERROR` on transport failure
///
/// # Safety
/// * `subscription` must be in `POLLING` state
/// * `buf` must point to writable memory of at least `buf_len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_try_recv_raw(
    subscription: *mut nros_subscription_t,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    if subscription.is_null() || (buf.is_null() && buf_len != 0) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_POLLING {
        return NROS_RET_BAD_SEQUENCE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let raw = &mut *(subscription_mut._opaque.as_mut_ptr()
            as *mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>);
        match raw.try_recv_raw() {
            Ok(Some(len)) => {
                let to_copy = len.min(buf_len);
                core::ptr::copy_nonoverlapping(raw.buffer().as_ptr(), buf, to_copy);
                to_copy as i32
            }
            Ok(None) => 0,
            Err(_) => NROS_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (subscription_mut, buf, buf_len);
        NROS_RET_ERROR
    }
}

/// # Safety
/// * `subscription` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_fini(
    subscription: *mut nros_subscription_t,
) -> nros_ret_t {
    validate_not_null!(subscription);

    let subscription = &mut *subscription;

    match subscription.state {
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED => {
            // L2: subscriber lives in the executor arena (if registered) —
            // just reset metadata.
        }
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_POLLING => {
            // L1: drop the inline RawSubscription so its Drop runs
            // (closes the underlying RMW subscriber).
            #[cfg(feature = "rmw-cffi")]
            {
                core::ptr::drop_in_place(subscription._opaque.as_mut_ptr()
                    as *mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>);
                subscription._opaque = [0u64; SUBSCRIPTION_OPAQUE_U64S];
            }
        }
        _ => return NROS_RET_NOT_INIT,
    }

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
