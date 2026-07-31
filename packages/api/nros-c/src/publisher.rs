//! Publisher API for nros C API.
//!
//! Publishers send messages to topics that subscribers can receive.

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use crate::{
    constants::{MAX_TOPIC_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN, PUBLISHER_OPAQUE_U64S},
    error::*,
    node::{nros_node_state_t, nros_node_t},
    qos::nros_qos_t,
};

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

/// Phase 189.M3 — rclc-style named publisher options.
///
/// Sits ALONGSIDE the QoS profile (rclc convention): QoS is passed
/// separately, this struct carries the non-QoS publisher-creation axes.
/// Publishers are not executor handles and therefore have no
/// scheduling-context binding — this struct is intentionally thin and
/// exists for rclc symmetry with [`nros_subscription_options_t`] plus
/// forward ABI headroom.
///
/// The struct contains only plain scalar fields — no pointers — so it is
/// safe to stack-allocate, memcpy, and pass across the FFI. Zero-init
/// (all fields 0) selects the default behaviour, identical to
/// `nros_publisher_init_with_qos`.
#[repr(C)]
#[derive(Default)]
pub struct nros_publisher_options_t {
    /// Reserved for future use; must be zero. Pads the struct to a
    /// non-empty layout and reserves room for later publisher-only axes
    /// (e.g. a future loan-pool hint) without an ABI break.
    pub _reserved: [u8; 4],
}

/// Get a zero-initialised [`nros_publisher_options_t`].
///
/// All fields default to "inherit"/"none". Callers populate only the
/// fields they want to override before passing the struct to
/// [`nros_publisher_init_with_options`].
#[unsafe(no_mangle)]
pub extern "C" fn nros_publisher_get_default_options() -> nros_publisher_options_t {
    nros_publisher_options_t::default()
}

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
    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::{Session, TopicInfo};

        // Phase 156 Sub-bug D — resolve session via the multi-Session
        // helper. Branches on whether the Node was bound via
        // `nros_executor_node_init` (sets `node.executor` + non-zero
        // `node.node_id`) or via legacy `nros_node_init` (sets
        // `node.support`). Single call covers both shapes; bridge
        // examples now reach the XRCE / DDS extra sessions instead of
        // hitting NROS_RET_NOT_INIT against a NULL support pointer.
        let (session, domain_id) = match crate::node::resolve_session_and_domain(node_ref) {
            Some(t) => t,
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

        // Phase 211.H (issue #52) — fold any plan qos_overrides for this
        // topic + publisher role into the profile before create, mirroring
        // Rust's `NodeHandle::create_publisher_with_qos`.
        let _qos_settings = crate::qos::apply_qos_overrides(
            _qos_settings,
            node_ref.qos_overrides,
            node_ref.qos_overrides_len,
            topic_str,
            crate::qos::QOS_OVERRIDE_ROLE_PUBLISHER,
        );

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

    #[cfg(not(feature = "rmw-cffi"))]
    {
        NROS_RET_ERROR
    }
}

/// Phase 189.M3 — initialize a publisher with custom QoS + named options.
///
/// rclc-style entry point: QoS is passed separately (`qos`, NULL =
/// default) and the non-QoS axes ride in `options` (NULL = defaults).
/// Equivalent to [`nros_publisher_init_with_qos`] today —
/// [`nros_publisher_options_t`] is currently a reserved, thin struct
/// (publishers have no scheduling-context binding) — but kept as a
/// distinct, additive entry point for rclc symmetry and forward ABI
/// headroom.
///
/// # Parameters
/// * `publisher` - Pointer to a zero-initialized publisher
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
/// * `qos` - Pointer to QoS settings (NULL for default)
/// * `options` - Pointer to publisher options (NULL for defaults)
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
/// * `qos` / `options` may be NULL or point to valid structs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_init_with_options(
    publisher: *mut nros_publisher_t,
    node: *const nros_node_t,
    type_info: *const nros_message_type_t,
    topic_name: *const c_char,
    qos: *const nros_qos_t,
    options: *const nros_publisher_options_t,
) -> nros_ret_t {
    // `options` carries no wired axis today (reserved). Validate it if
    // non-NULL so a future field gain doesn't silently accept garbage,
    // then delegate to the QoS path.
    if !options.is_null() {
        let _opts = &*options;
        // No-op: every field is currently reserved.
    }
    nros_publisher_init_with_qos(publisher, node, type_info, topic_name, qos)
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

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        let data_slice = core::slice::from_raw_parts(data, len);

        match pub_handle.publish_raw(data_slice) {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        NROS_RET_ERROR
    }
}

/// Phase 124.E.1 — streamed publish.
///
/// Two callbacks: `size_cb` reports the total payload length once,
/// `chunk_cb` fills the slot in chunks. Backends that support
/// streaming land each chunk directly in their outbound buffer;
/// backends that don't fall through to a stack-allocated staging
/// buffer (capped at ~4 KiB) + a single `publish_raw`.
///
/// # Parameters
/// * `publisher` — initialized publisher
/// * `size_cb` — invoked once; writes the total byte count to
///   `*out_total_len`
/// * `chunk_cb` — invoked repeatedly; writes up to `cap` bytes
///   starting at `out_buf`, reports the count via `*out_written`.
///   `*out_written == 0` signals EOF
/// * `user_ctx` — opaque pointer passed through to both callbacks
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NROS_RET_NOT_INIT` if not initialised
/// * `NROS_RET_PUBLISH_FAILED` on backend failure
/// * `NROS_RET_BUFFER_TOO_SMALL` if the fallback's staging buffer
///   is exceeded
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialised publisher.
/// * The callbacks MUST NOT return references that outlive the
///   call; `user_ctx` is valid only for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_publish_streamed(
    publisher: *const nros_publisher_t,
    size_cb: Option<unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut c_void)>,
    chunk_cb: Option<
        unsafe extern "C" fn(
            out_buf: *mut u8,
            cap: usize,
            out_written: *mut usize,
            user_ctx: *mut c_void,
        ),
    >,
    user_ctx: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(publisher);
    let size_cb = match size_cb {
        Some(f) => f,
        None => return NROS_RET_INVALID_ARGUMENT,
    };
    let chunk_cb = match chunk_cb {
        Some(f) => f,
        None => return NROS_RET_INVALID_ARGUMENT,
    };

    let publisher = &*publisher;
    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;
        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        // SAFETY: this C entry point is unsafe; callers must keep
        // `user_ctx` valid for the synchronous callback sequence.
        match unsafe { pub_handle.publish_streamed(size_cb, chunk_cb, user_ctx) } {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (size_cb, chunk_cb, user_ctx);
        NROS_RET_ERROR
    }
}

/// Phase 108.B.7 — manually assert this publisher's liveliness.
///
/// Required for entities created with QoS `liveliness_kind =
/// NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC` or `MANUAL_BY_NODE`. No-op for
/// `AUTOMATIC` / `NONE`. Backends that don't implement manual
/// assertion (XRCE-DDS, zenoh-pico, uORB today) treat this as a no-op
/// and return `NROS_RET_OK`.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if publisher is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
/// * `NROS_RET_PUBLISH_FAILED` on backend failure
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_assert_liveliness(
    publisher: *const nros_publisher_t,
) -> nros_ret_t {
    validate_not_null!(publisher);

    let publisher = &*publisher;

    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        match pub_handle.assert_liveliness() {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        NROS_RET_ERROR
    }
}

// ============================================================================
// Phase 124.A.6 — zero-copy publisher loan / commit / discard
// ============================================================================

/// Phase 124.A.6 — loan a writable slot from the publisher's outbound
/// buffer (zero-copy publish path).
///
/// On success, `*out_buf` points at `*out_cap` writable bytes the
/// caller fills in place. Pass `*out_token` back to
/// [`nros_publisher_commit`] (to send) or [`nros_publisher_discard`]
/// (to abandon). The slot's bytes are valid until commit / discard
/// runs OR the publisher is finalised — whichever comes first. The
/// caller is responsible for matching every loan with exactly one
/// commit OR discard.
///
/// Falls back to a heap-allocated staging buffer when the active
/// backend's vtable doesn't expose a native loan slot — the wire
/// payload still takes a single memcpy at commit time. `requested_len`
/// is the minimum capacity; `*out_cap` may exceed it.
///
/// # Returns
/// * `NROS_RET_OK` — slot reserved.
/// * `NROS_RET_TRY_AGAIN` (`-15`) — backend has no slot available;
///   retry later or use a non-loan publish path.
/// * `NROS_RET_INVALID_ARGUMENT` on NULL pointers or zero `requested_len`.
/// * `NROS_RET_NOT_INIT` if publisher isn't initialised.
///
/// # Safety
/// * All pointers must be valid.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_loan(
    publisher: *const nros_publisher_t,
    requested_len: usize,
    out_buf: *mut *mut u8,
    out_cap: *mut usize,
    out_token: *mut *mut core::ffi::c_void,
) -> nros_ret_t {
    validate_not_null!(publisher, out_buf, out_cap, out_token);
    if requested_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    let publisher = &*publisher;
    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );

    use nros_rmw::SlotLending;
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    match pub_handle.try_lend_slot(requested_len) {
        Ok(Some(slot)) => {
            // SAFETY: erase the lifetime — caller is contractually
            // responsible for commit/discard before the publisher
            // dies. Box the slot so we have a stable token across
            // the FFI boundary.
            let mut slot: nros::internals::RmwSlot<'static> = core::mem::transmute(slot);
            let buf_ptr = slot.as_mut().as_mut_ptr();
            let cap = slot.as_mut().len();
            let boxed = alloc::boxed::Box::new(slot);
            *out_buf = buf_ptr;
            *out_cap = cap;
            *out_token = alloc::boxed::Box::into_raw(boxed) as *mut core::ffi::c_void;
            NROS_RET_OK
        }
        Ok(None) => NROS_RET_TRY_AGAIN,
        Err(_) => NROS_RET_PUBLISH_FAILED,
    }
}

/// Phase 124.A.6 — commit a previously-loaned slot. Sends the slot's
/// `actual_len` bytes via the active backend.
///
/// `token` MUST come from a prior `nros_publisher_loan` on the SAME
/// publisher; consuming it (commit OR discard) is mandatory.
///
/// # Safety
/// * `publisher` must be the same publisher the token was loaned from.
/// * `token` must not be NULL and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_commit(
    publisher: *const nros_publisher_t,
    token: *mut core::ffi::c_void,
    actual_len: usize,
) -> nros_ret_t {
    validate_not_null!(publisher, token);
    let publisher = &*publisher;
    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );
    use nros_rmw::SlotLending;
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    let mut slot: alloc::boxed::Box<nros::internals::RmwSlot<'static>> =
        alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwSlot<'static>);
    slot.set_len(actual_len);
    match pub_handle.commit_slot(*slot) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_PUBLISH_FAILED,
    }
}

/// Phase 124.A.6 — abandon a previously-loaned slot without sending.
///
/// `token` MUST come from a prior `nros_publisher_loan` on the SAME
/// publisher; consuming it (commit OR discard) is mandatory.
///
/// # Safety
/// * `publisher` must be the same publisher the token was loaned from.
/// * `token` must not be NULL and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_discard(
    publisher: *const nros_publisher_t,
    token: *mut core::ffi::c_void,
) -> nros_ret_t {
    validate_not_null!(publisher, token);
    let publisher = &*publisher;
    validate_state!(
        publisher,
        nros_publisher_state_t::NROS_PUBLISHER_STATE_INITIALIZED
    );
    // SAFETY: token was a Box::into_raw of `RmwSlot<'static>` from a
    // prior `nros_publisher_loan`. Reconstitute it and drop —
    // CffiSlot::drop fires the backend's pub_discard (or reclaims the
    // arena staging buffer).
    let _slot: alloc::boxed::Box<nros::internals::RmwSlot<'static>> =
        alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwSlot<'static>);
    NROS_RET_OK
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
    #[cfg(feature = "rmw-cffi")]
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
