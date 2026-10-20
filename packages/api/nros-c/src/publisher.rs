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
    node::{rcl_node_t, rclc_node_state_t},
    qos::rclc_qos_t,
};

/// A type-erased CDR serialiser for one message type (phase-492 W8).
///
/// Writes `msg` (a pointer to the generated struct) into `buffer[..capacity]`
/// with its 4-byte encapsulation header, stores the byte count in `*out_len`,
/// and returns 0; any failure (buffer too small, a field past its `.msg`
/// bound, a NULL `data` with a nonzero `size`) returns nonzero. Never
/// allocates.
pub type rclc_message_serialize_fn_t = Option<
    unsafe extern "C" fn(
        msg: *const c_void,
        buffer: *mut u8,
        capacity: usize,
        out_len: *mut usize,
    ) -> i32,
>;

/// A type-erased CDR deserialiser for one message type (phase-492 W8).
///
/// Reads `buffer[..len]` into `msg` (a pointer to the generated struct, its
/// strings and sequences wired to caller memory) and returns 0; a malformed
/// sample or a field that does not fit its `capacity` returns nonzero and the
/// field is not written. Never allocates.
pub type rclc_message_deserialize_fn_t =
    Option<unsafe extern "C" fn(buffer: *const u8, len: usize, msg: *mut c_void) -> i32>;

/// Message type descriptor: what `ROSIDL_GET_MSG_TYPE_SUPPORT(pkg, msg, Name)`
/// yields.
///
/// The generated `<pkg>__msg__<Name>__nros_type` is a `static const` of this
/// type in read-only memory. It carries the type's NAME and HASH (what the RMW
/// needs to create an entity) and, since phase-492 W8 (RFC-0089
/// "`ROSIDL_GET_*_TYPE_SUPPORT` adopted as a pass-through"), its CODEC, so
/// `rcl_publish` and `rclc_executor_add_subscription_with_context` work on the
/// generated struct with rclc's arguments. It is NOT rosidl's
/// `rosidl_message_type_support_t`, which stays undeclared: that struct's
/// members (`typesupport_identifier`, `data`, `func`) are a dispatcher this
/// descriptor does not have.
#[repr(C)]
pub struct rclc_message_type_t {
    /// Type name (e.g., "std_msgs::msg::dds_::Int32")
    pub type_name: *const c_char,
    /// Type hash (RIHS format)
    pub type_hash: *const c_char,
    /// Largest CDR payload this type can arrive as, encapsulation header
    /// included (the generated `__RX_MAX_SERIALIZED_SIZE`). 0 = unbounded or
    /// unknown.
    pub serialized_size_max: usize,
    /// The type's serialiser; NULL in a descriptor built from names alone, which
    /// `rcl_publish` refuses with `RCL_RET_UNSUPPORTED`.
    pub serialize: rclc_message_serialize_fn_t,
    /// The type's deserialiser; NULL in a descriptor built from names alone,
    /// which `rclc_executor_add_subscription_with_context` refuses.
    pub deserialize: rclc_message_deserialize_fn_t,
}

/// Service type descriptor: what `ROSIDL_GET_SRV_TYPE_SUPPORT(pkg, srv, Name)`
/// yields (phase-492 W8).
///
/// Generated as `<pkg>__srv__<Name>__nros_type`. The four codec slots are the
/// request and response parts' serialisers, with the
/// [`rclc_message_serialize_fn_t`] / [`rclc_message_deserialize_fn_t`]
/// contracts; NULL where the descriptor was built from names alone.
#[repr(C)]
pub struct rclc_service_type_t {
    /// Type name (e.g., "example_interfaces::srv::dds_::AddTwoInts_")
    pub type_name: *const c_char,
    /// Type hash (RIHS format)
    pub type_hash: *const c_char,
    /// The request part's serialiser.
    pub request_serialize: rclc_message_serialize_fn_t,
    /// The request part's deserialiser.
    pub request_deserialize: rclc_message_deserialize_fn_t,
    /// The response part's serialiser.
    pub response_serialize: rclc_message_serialize_fn_t,
    /// The response part's deserialiser.
    pub response_deserialize: rclc_message_deserialize_fn_t,
}

/// upstream rmw's publisher pre-allocation handle, declared INCOMPLETE
/// (phase-492 W8).
///
/// `rcl_publish(&pub, &msg, NULL)` compiles because a pointer to an incomplete
/// type is fine; declaring one (`rmw_publisher_allocation_t a;`) does not,
/// because nano-ros has no pre-allocation to hand out: the transmit buffer is
/// already bounded at build time. Pass NULL.
pub struct rmw_publisher_allocation_t {
    _private: [u8; 0],
}

/// Publisher state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_publisher_state_t {
    /// Not initialized
    RCLC_PUBLISHER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    RCLC_PUBLISHER_STATE_INITIALIZED = 1,
    /// Shutdown
    RCLC_PUBLISHER_STATE_SHUTDOWN = 2,
}

/// Publisher structure.
#[repr(C)]
pub struct rcl_publisher_t {
    /// Current state
    pub state: rclc_publisher_state_t,
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
    pub node: crate::node::rclc_node_ref_t,
    /// phase-492 W8 — the descriptor this publisher was created with, kept so
    /// `rcl_publish` can serialise through its codec. Descriptors are generated
    /// `static const`s, so the pointer outlives the publisher.
    pub type_support: *const rclc_message_type_t,
    /// Inline opaque storage for the RMW publisher handle.
    /// Avoids heap allocation — managed by rclc_publisher_init_default/fini.
    pub _opaque: [u64; PUBLISHER_OPAQUE_U64S],
}

impl Default for rcl_publisher_t {
    fn default() -> Self {
        Self {
            state: rclc_publisher_state_t::RCLC_PUBLISHER_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            node: crate::node::rclc_node_ref_t::none(),
            type_support: ptr::null(),
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
/// exists for rclc symmetry with `rcl_subscription_options_t` plus
/// forward ABI headroom.
///
/// The struct contains only plain scalar fields — no pointers — so it is
/// safe to stack-allocate, memcpy, and pass across the FFI. Zero-init
/// (all fields 0) selects the default behaviour, identical to
/// `rclc_publisher_init_with_qos`.
#[repr(C)]
#[derive(Default)]
pub struct rcl_publisher_options_t {
    /// Reserved for future use; must be zero. Pads the struct to a
    /// non-empty layout and reserves room for later publisher-only axes
    /// (e.g. a future loan-pool hint) without an ABI break.
    pub _reserved: [u8; 4],
}

/// Get a zero-initialised [`rcl_publisher_options_t`].
///
/// All fields default to "inherit"/"none". Callers populate only the
/// fields they want to override before passing the struct to
/// [`rclc_publisher_init_with_options`].
#[unsafe(no_mangle)]
pub extern "C" fn rcl_publisher_get_default_options() -> rcl_publisher_options_t {
    rcl_publisher_options_t::default()
}

/// Get a zero-initialized publisher.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_zero_initialized_publisher() -> rcl_publisher_t {
    rcl_publisher_t::default()
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
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `RCL_RET_NOT_INIT` if node is not initialized
/// * `RCL_RET_ERROR` on initialization failure
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_init_default(
    publisher: *mut rcl_publisher_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
) -> rcl_ret_t {
    rclc_publisher_init_with_qos(publisher, node, type_info, topic_name, ptr::null())
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
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `RCL_RET_NOT_INIT` if node is not initialized
/// * `RCL_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_init_with_qos(
    publisher: *mut rcl_publisher_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
    qos: *const rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(publisher, node, type_info, topic_name);

    let publisher = &mut *publisher;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // Copy topic name (required — empty rejected)
    publisher.topic_name_len = crate::util::copy_cstr_into(topic_name, &mut publisher.topic_name);
    if publisher.topic_name_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    publisher.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut publisher.type_name);
    publisher.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut publisher.type_hash);

    // Store node reference
    publisher.node = unsafe { crate::node::node_ref_of(node) };
    // phase-492 W8 — the codec `rcl_publish` serialises through.
    publisher.type_support = type_info;

    // Resolve the SOURCE topic name to its wire name — ROS 2 expansion plus
    // this node's launch remap rules.
    //
    // A publisher is created HERE, not at executor registration, so it is one
    // of the two C entities that never reached the resolution seam every other
    // kind goes through (`nros_executor_add_*`). On a `/ns` node a publisher
    // and a subscription created with the same string therefore landed on
    // different keys and never connected, and a launch remap moved the
    // subscription and not the publisher — both returning `RCL_RET_OK`.
    // `TopicInfo::with_namespace` below does NOT close it: the zenoh key is
    // built from `TopicInfo::name` alone.
    //
    // Resolved BEFORE `resolve_session_and_domain`: both reach the executor
    // through `node.executor`, and taking the second `&mut` while the first
    // session borrow is live would alias. The order is also the better
    // contract — an unresolvable name is INVALID_ARGUMENT rather than
    // whatever the session lookup happens to answer first.
    let _resolved_topic = {
        let source =
            core::str::from_utf8_unchecked(&publisher.topic_name[..publisher.topic_name_len]);
        match crate::node::resolve_entity_name_on_node(node_ref, source) {
            Some(r) => r,
            None => return RCL_RET_INVALID_ARGUMENT,
        }
    };

    // Get QoS settings
    let _qos_settings = if qos.is_null() {
        crate::qos::RCLC_QOS_DEFAULT.to_qos_settings()
    } else {
        (*qos).to_qos_settings()
    };

    // Create the internal publisher
    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::{Session, TopicInfo};

        // Phase 156 Sub-bug D — resolve session via the multi-Session
        // helper. Branches on whether the Node was bound via
        // `rclc_executor_node_init` (sets `node.executor` + non-zero
        // `node.node_id`) or via legacy `rclc_node_init_default` (sets
        // `node.support`). Single call covers both shapes; bridge
        // examples now reach the XRCE / DDS extra sessions instead of
        // hitting RCL_RET_NOT_INIT against a NULL support pointer.
        let (session, domain_id) = match crate::node::resolve_session_and_domain(node_ref) {
            Some(t) => t,
            None => return RCL_RET_NOT_INIT,
        };

        // Build the topic key expression for ROS 2 compatibility. `topic_str`
        // is the SOURCE spelling and is used only for the QoS-override lookup
        // below (the deploy plan writes overrides against launch names, which
        // is the same choice `nros-cpp/src/publisher.rs` records); the wire
        // name is `_resolved_topic`.
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

        // Build TopicInfo from the RESOLVED name — see the resolution above.
        let topic_info = TopicInfo::new(_resolved_topic.as_str(), type_str, type_hash_str)
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

        // issue 1608 -- the publisher half of the declared-QoS check. A C
        // publisher has no compile-time seam that took the declaration, so it
        // is HONOURED here (raised to a stronger declared policy, refused when
        // it asks for more than the contract declares) rather than
        // strict-checked. See `nros_node::declared_qos::honour_publisher`.
        let _qos_settings = match nros_node::declared_qos::honour_publisher(
            topic_info.type_name,
            topic_info.name,
            _qos_settings,
        ) {
            Ok(q) => q,
            Err(_) => return RCL_RET_ERROR,
        };

        // Create publisher — write handle directly into inline opaque storage
        match session.create_publisher(&topic_info, _qos_settings) {
            Ok(pub_handle) => {
                core::ptr::write(
                    publisher._opaque.as_mut_ptr() as *mut nros::internals::RmwPublisher,
                    pub_handle,
                );
            }
            Err(_) => return RCL_RET_ERROR,
        }

        publisher.state = rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED;
        RCL_RET_OK
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_ERROR
    }
}

/// Phase 189.M3 — initialize a publisher with custom QoS + named options.
///
/// rclc-style entry point: QoS is passed separately (`qos`, NULL =
/// default) and the non-QoS axes ride in `options` (NULL = defaults).
/// Equivalent to [`rclc_publisher_init_with_qos`] today —
/// [`rcl_publisher_options_t`] is currently a reserved, thin struct
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
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `RCL_RET_NOT_INIT` if node is not initialized
/// * `RCL_RET_ERROR` on initialization failure
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
/// * `qos` / `options` may be NULL or point to valid structs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_init_with_options(
    publisher: *mut rcl_publisher_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
    qos: *const rclc_qos_t,
    options: *const rcl_publisher_options_t,
) -> rcl_ret_t {
    // `options` carries no wired axis today (reserved). Validate it if
    // non-NULL so a future field gain doesn't silently accept garbage,
    // then delegate to the QoS path.
    if !options.is_null() {
        let _opts = &*options;
        // No-op: every field is currently reserved.
    }
    rclc_publisher_init_with_qos(publisher, node, type_info, topic_name, qos)
}

/// Publish raw CDR-serialized data.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
/// * `data` - Pointer to CDR-serialized message data
/// * `len` - Length of data in bytes
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any pointer is NULL or len is 0
/// * `RCL_RET_NOT_INIT` if publisher is not initialized
/// * `RCLC_RET_PUBLISH_FAILED` on publish failure
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher
/// * `data` must be a valid pointer to `len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publish_raw(
    publisher: *const rcl_publisher_t,
    data: *const u8,
    len: usize,
) -> rcl_ret_t {
    validate_not_null!(publisher, data);
    if len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let publisher = &*publisher;

    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        let data_slice = core::slice::from_raw_parts(data, len);

        match pub_handle.publish_raw(data_slice) {
            Ok(()) => RCL_RET_OK,
            Err(_) => RCLC_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_ERROR
    }
}

/// Publish a message — rcl's `rcl_publish`, serialising through the
/// descriptor this publisher was created with (phase-492 W8).
///
/// ```c
/// std_msgs__msg__String msg;          // wired by __init_in, or by hand
/// rcl_publish(&publisher, &msg, NULL);
/// ```
///
/// `ros_message` points at the generated struct of the publisher's type. It is
/// serialised by the descriptor's codec into a transmit buffer on this call's
/// stack whose size is BOUNDED AT BUILD TIME, then handed to the backend:
///
/// * the type's own receive bound (`serialized_size_max`) picks the smaller
///   256-byte class when it fits;
/// * otherwise, or when the sample does not fit the smaller class (a caller
///   that pointed a string at a buffer larger than `__Storage` sizes), the
///   image's message buffer size (`NROS_SUBSCRIPTION_BUFFER_SIZE`, the bound
///   the image derives over every type it carries).
///
/// No heap, ever (RFC-0105 D10): nothing here, in the serialiser or in the
/// generated struct allocates. A sample larger than the image's message buffer
/// is refused with `RCL_RET_ERROR`; `rclc_publisher_publish_streamed` is the
/// entry point for payloads past that bound.
///
/// # Parameters
/// * `publisher` — an initialised publisher
/// * `ros_message` — the message, of the publisher's type
/// * `allocation` — MUST be NULL. `rmw_publisher_allocation_t` is declared
///   incomplete: nano-ros has no pre-allocation to offer.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL `publisher` / `ros_message`, or a
///   non-NULL `allocation`
/// * `RCL_RET_NOT_INIT` — the publisher is not initialised
/// * `RCL_RET_UNSUPPORTED` — the publisher's descriptor carries no serialiser
///   (one built from type names alone); use `rclc_publish_raw`
/// * `RCL_RET_ERROR` — the message did not serialise (a field past its `.msg`
///   bound, a NULL `data` with nonzero `size`, or larger than the image's
///   message buffer)
/// * `RCLC_RET_PUBLISH_FAILED` — the backend refused the sample
///
/// # Safety
/// * `publisher` must point at a valid publisher.
/// * `ros_message` must point at a message of the publisher's type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_publish(
    publisher: *const rcl_publisher_t,
    ros_message: *const c_void,
    allocation: *mut rmw_publisher_allocation_t,
) -> rcl_ret_t {
    validate_not_null!(publisher, ros_message);
    if !allocation.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let publisher_ref = &*publisher;
    validate_state!(
        publisher_ref,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );
    let Some(type_info) = publisher_ref.type_support.as_ref() else {
        return RCL_RET_UNSUPPORTED;
    };
    let Some(serialize) = type_info.serialize else {
        return RCL_RET_UNSUPPORTED;
    };

    const SMALL: usize = 256;
    const IMAGE: usize = crate::config::MESSAGE_BUFFER_SIZE;
    let bound = type_info.serialized_size_max;
    if bound != 0 && bound <= SMALL && SMALL < IMAGE {
        if let Some(rc) = publish_staged::<SMALL>(publisher, serialize, ros_message) {
            return rc;
        }
    }
    publish_staged::<IMAGE>(publisher, serialize, ros_message).unwrap_or(RCL_RET_ERROR)
}

/// Serialise `msg` into an `N`-byte stack buffer and publish it; `None` when it
/// does not serialise into `N` bytes. A separate non-inlined frame so only the
/// chosen class's buffer is ever on the stack.
#[inline(never)]
unsafe fn publish_staged<const N: usize>(
    publisher: *const rcl_publisher_t,
    serialize: unsafe extern "C" fn(*const c_void, *mut u8, usize, *mut usize) -> i32,
    msg: *const c_void,
) -> Option<rcl_ret_t> {
    let mut buf = [0u8; N];
    let mut len = 0usize;
    // SAFETY: `serialize` is the descriptor's codec for this publisher's type;
    // it writes at most `N` bytes into `buf` and reports the count.
    let rc = unsafe { serialize(msg, buf.as_mut_ptr(), N, &mut len) };
    if rc != 0 || len == 0 || len > N {
        return None;
    }
    // SAFETY: `publisher` was validated by the caller; `buf[..len]` is live.
    Some(unsafe { rclc_publish_raw(publisher, buf.as_ptr(), len) })
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
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `RCL_RET_NOT_INIT` if not initialised
/// * `RCLC_RET_PUBLISH_FAILED` on backend failure — INCLUDING a payload
///   larger than the fallback's ~4 KiB staging buffer, and a `chunk_cb`
///   that reports 0 written before `*out_total_len` bytes have been
///   produced. This line used to name a `NROS_RET_`-prefixed
///   `BUFFER_TOO_SMALL` (issue 1126): `rcl_ret_t` has never defined
///   one — the constant is `NROS_RMW_RET_BUFFER_TOO_SMALL`, on the RMW ABI
///   (`nros_rmw_ret_t`), and a C caller writing the documented comparison
///   got an undeclared identifier with nothing pointing back here. The
///   backend errors collapse to one code on the way out, so the two
///   causes are NOT distinguishable by the caller; for this entry point
///   that is tolerable, since the caller sized the payload it streamed.
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialised publisher.
/// * The callbacks MUST NOT return references that outlive the
///   call; `user_ctx` is valid only for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_publish_streamed(
    publisher: *const rcl_publisher_t,
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
) -> rcl_ret_t {
    validate_not_null!(publisher);
    let size_cb = match size_cb {
        Some(f) => f,
        None => return RCL_RET_INVALID_ARGUMENT,
    };
    let chunk_cb = match chunk_cb {
        Some(f) => f,
        None => return RCL_RET_INVALID_ARGUMENT,
    };

    let publisher = &*publisher;
    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;
        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        // SAFETY: this C entry point is unsafe; callers must keep
        // `user_ctx` valid for the synchronous callback sequence.
        match unsafe { pub_handle.publish_streamed(size_cb, chunk_cb, user_ctx) } {
            Ok(()) => RCL_RET_OK,
            Err(_) => RCLC_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (size_cb, chunk_cb, user_ctx);
        RCL_RET_ERROR
    }
}

/// Phase 108.B.7 — manually assert this publisher's liveliness.
///
/// Required for entities created with QoS `liveliness_kind =
/// RCLC_QOS_LIVELINESS_MANUAL_BY_TOPIC` or `MANUAL_BY_NODE`; there is
/// nothing to assert under `AUTOMATIC` / `NONE`, and those answer
/// `RCL_RET_OK` on every backend.
///
/// **A BACKEND THAT CANNOT ASSERT REMOTELY ANSWERS `RCL_RET_UNSUPPORTED`,
/// and this doc said the opposite until the phase-467 RMW gap-closure design
/// study's Row 7.** It read "Backends that don't implement manual assertion
/// (XRCE-DDS, zenoh-pico, uORB today) treat this as a no-op and return
/// `RCL_RET_OK`" — an `OK` from a call that put nothing on the wire, which
/// is the one thing a caller uses this function to find out. cyclonedds does
/// implement it (`dds_assert_liveliness` renews the writer lease and sends a
/// Heartbeat); zenoh-pico, XRCE-DDS and uORB have no per-topic lease and now
/// say so.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
///
/// # Returns
/// * `RCL_RET_OK` — asserted, or there was nothing to assert
/// * `RCL_RET_INVALID_ARGUMENT` if publisher is NULL
/// * `RCL_RET_NOT_INIT` if not initialized
/// * `RCL_RET_UNSUPPORTED` — a manual liveliness kind on a backend with no
///   per-topic lease. NOT a failure of this call; the entity is fine.
/// * `RCLC_RET_PUBLISH_FAILED` on backend failure
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_publisher_assert_liveliness(
    publisher: *const rcl_publisher_t,
) -> rcl_ret_t {
    validate_not_null!(publisher);

    let publisher = &*publisher;

    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        match pub_handle.assert_liveliness() {
            Ok(()) => RCL_RET_OK,
            Err(nros_rmw::TransportError::Unsupported) => RCL_RET_UNSUPPORTED,
            Err(_) => RCLC_RET_PUBLISH_FAILED,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_ERROR
    }
}

/// The number of bytes in a publisher GID — upstream's
/// `RMW_GID_STORAGE_SIZE`, and the size of the buffer
/// [`rclc_publisher_get_gid`] fills.
pub const NROS_PUBLISHER_GID_SIZE: usize = 24;

/// This publisher's own global identifier — upstream
/// `rmw_get_gid_for_publisher`.
///
/// The phase-467 RMW gap-closure design study's Q1. Writes
/// `NROS_PUBLISHER_GID_SIZE` (24) bytes into `out_gid` and touches nothing on
/// failure.
///
/// **Three bounds, each of which a caller will otherwise assume away:**
///
/// * **How many of the 24 bytes MEAN anything is a backend property.** A
///   backend whose identity is narrower zero-extends into the tail, so two
///   gids naming one entity compare equal. Gids from two different backends
///   in one image are never comparable.
/// * **Comparable with the gid on a received sample** on zenoh and Cyclone,
///   which derive both from one source (issue 1495).
/// * **An all-zero gid is never returned as an answer.** A backend with no
///   identity for this publisher reports `RCL_RET_UNSUPPORTED`, because
///   all-zero is what an unwritten buffer holds and the two must not read
///   alike.
///
/// # Parameters
/// * `publisher` — pointer to an initialized publisher
/// * `out_gid` — buffer of at least `gid_capacity` bytes, filled on
///   `RCL_RET_OK`
/// * `gid_capacity` — how many bytes `out_gid` can hold. CHECKED, not
///   assumed. The C surface has no `nros_gid_t`, so the out-param is a bare
///   `uint8_t *` that carries no width of its own, and the callee's only
///   alternative to a capacity argument is to write 24 bytes on the caller's
///   word. The width is also not a constant that has held still: the Rust
///   `PUBLISHER_GID_SIZE` this mirrors was 16 until the same study's Q1(a).
///
/// # Returns
/// * `RCL_RET_OK` — `out_gid[0 .. NROS_PUBLISHER_GID_SIZE]` written
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument, or
///   `gid_capacity < NROS_PUBLISHER_GID_SIZE`
/// * `RCL_RET_NOT_INIT` if not initialized
/// * `RCL_RET_UNSUPPORTED` — this backend has no identity for this
///   publisher (XRCE-DDS, uORB)
/// * `RCL_RET_ERROR` — the backend was asked and failed
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher.
/// * `out_gid` must be writable for `gid_capacity` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_get_gid(
    publisher: *const rcl_publisher_t,
    out_gid: *mut u8,
    gid_capacity: usize,
) -> rcl_ret_t {
    validate_not_null!(publisher, out_gid);
    if gid_capacity < NROS_PUBLISHER_GID_SIZE {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let publisher = &*publisher;

    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::Publisher;

        let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
        match pub_handle.get_gid() {
            Ok(gid) => {
                ptr::copy_nonoverlapping(gid.as_ptr(), out_gid, NROS_PUBLISHER_GID_SIZE);
                RCL_RET_OK
            }
            Err(nros_rmw::TransportError::Unsupported) => RCL_RET_UNSUPPORTED,
            Err(_) => RCL_RET_ERROR,
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = out_gid;
        RCL_RET_UNSUPPORTED
    }
}

// ============================================================================
// Phase 124.A.6 — zero-copy publisher loan / commit / discard
// ============================================================================

/// Phase 124.A.6 — loan a writable slot from the publisher's outbound
/// buffer (zero-copy publish path).
///
/// # AVAILABILITY — read this before calling
///
/// This symbol exists **only** in a nano-ros built with the `lending`
/// cargo feature, and **no shipped configuration enables it**: the
/// string `lending` appears zero times under `cmake/` and `zephyr/`,
/// `nros-rmw-zenoh-staticlib` — the crate every C/C++ and RTOS image
/// links — declares no forwarder for it, and the only crate in the tree
/// that turns it on is the `nros-tests` test harness.
///
/// Its presence in this header is therefore **not evidence that it can
/// be linked.** cbindgen emits every declaration unconditionally here,
/// by deliberate repo-wide policy (`packages/api/nros-c/cbindgen.toml`
/// `[defines]`, empty on purpose), so a `#[cfg]`-gated Rust symbol still
/// appears. Calling this against a default build compiles and then fails
/// at LINK with an undefined symbol.
///
/// The supported zero-copy surface is `rclc_publisher_publish_streamed`
/// and its receive twin `process_raw_in_place`: no feature, no token, no
/// arena, no size ceiling, and the XRCE and zenoh backends fill them
/// natively while all three NULL the loan trio. See issue 0814.
///
/// On success, `*out_buf` points at `*out_cap` writable bytes the
/// caller fills in place. Pass `*out_token` back to
/// [`rclc_publisher_commit`] (to send) or [`rclc_publisher_discard`]
/// (to abandon). The slot's bytes are valid until commit / discard
/// runs OR the publisher is finalised — whichever comes first. The
/// caller is responsible for matching every loan with exactly one
/// commit OR discard.
///
/// Falls back to a heap-allocated staging buffer when the active
/// backend's vtable doesn't expose a native loan slot — the wire
/// payload still takes a single memcpy at commit time. `requested_len`
/// is the minimum capacity; `*out_cap` may exceed it. That fallback
/// needs a heap: in a build without `alloc` there is nothing to hand
/// out, and the answer is `RCLC_RET_NOT_ALLOWED` (see below).
///
/// # Returns
/// * `RCL_RET_OK` — slot reserved.
/// * `RCLC_RET_TRY_AGAIN` (`-14`) — no slot available *right now*.
///   TRANSIENT: retry later, or use a non-loan publish path.
/// * `RCLC_RET_NOT_ALLOWED` (`-12`) — this publisher cannot loan at
///   all. PERMANENT (issue 0814): the backend's vtable exposes no loan
///   slot and this build has no heap for the staging fallback, and
///   neither fact can change while the publisher lives. Do NOT retry —
///   take a non-loan path. `rclc_publisher_publish_streamed` is the
///   better one: it needs no token, no arena and no heap, and the XRCE
///   and zenoh backends fill it natively.
/// * `RCL_RET_INVALID_ARGUMENT` on NULL pointers or zero `requested_len`.
/// * `RCL_RET_NOT_INIT` if publisher isn't initialised.
///
/// # Safety
/// * All pointers must be valid.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_loan(
    publisher: *const rcl_publisher_t,
    requested_len: usize,
    out_buf: *mut *mut u8,
    out_cap: *mut usize,
    out_token: *mut *mut core::ffi::c_void,
) -> rcl_ret_t {
    validate_not_null!(publisher, out_buf, out_cap, out_token);
    if requested_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let publisher = &*publisher;
    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    // Issue 0812 — `try_lend_raw` hands back the BACKEND's own token, so
    // nothing has to be stored on this side of the FFI boundary. This used
    // to `Box` a lifetime-erased slot per loan: a malloc on the path whose
    // entire purpose is removing copies, and a hard `alloc` dependency on a
    // surface a heap-free image is meant to be able to use.
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    match pub_handle.try_lend_raw(requested_len) {
        Ok(Some((buf_ptr, cap, token))) => {
            *out_buf = buf_ptr;
            *out_cap = cap;
            // issue 0814 — issue 0812's refactor retyped the backend token
            // from `*mut c_void` to `*mut rmw_loan_token_t`; the FFI keeps the
            // `void *` spelling because that is what the C ABI declares, so the
            // conversion is explicit HERE rather than absent (which is what
            // stopped this crate compiling under `lending`).
            *out_token = token as *mut core::ffi::c_void;
            RCL_RET_OK
        }
        Ok(None) => RCLC_RET_TRY_AGAIN,
        // issue 0814 — through the crate's ONE mapper, not a second
        // spelling. The blanket `Err(_) => RCLC_RET_PUBLISH_FAILED` this
        // replaces collapsed every backend fault into "the publish
        // failed", which is exactly wrong for the case that motivated
        // the fix: a heap-free image whose backend cannot lend now
        // reports `Unsupported`, and the caller needs to read that as
        // PERMANENT (`RCLC_RET_NOT_ALLOWED`) rather than as a publish
        // that might work next time.
        Err(e) => crate::support::transport_error_to_ret(e),
    }
}

/// Phase 124.A.6 — commit a previously-loaned slot. Sends the slot's
/// `actual_len` bytes via the active backend.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `rclc_publisher_loan`. Issue 0814.
///
/// `token` MUST come from a prior `rclc_publisher_loan` on the SAME
/// publisher; consuming it (commit OR discard) is mandatory.
///
/// # Safety
/// * `publisher` must be the same publisher the token was loaned from.
/// * `token` must not be NULL and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_commit(
    publisher: *const rcl_publisher_t,
    token: *mut core::ffi::c_void,
    actual_len: usize,
) -> rcl_ret_t {
    validate_not_null!(publisher, token);
    let publisher = &*publisher;
    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    // SAFETY: `token` is the backend token a prior `rclc_publisher_loan`
    // handed out on this publisher; the caller's contract is that it is
    // still outstanding and is consumed here exactly once.
    match pub_handle.commit_raw(token as *mut _, actual_len) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCLC_RET_PUBLISH_FAILED,
    }
}

/// Phase 124.A.6 — abandon a previously-loaned slot without sending.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `rclc_publisher_loan`. Issue 0814.
///
/// `token` MUST come from a prior `rclc_publisher_loan` on the SAME
/// publisher; consuming it (commit OR discard) is mandatory.
///
/// # Safety
/// * `publisher` must be the same publisher the token was loaned from.
/// * `token` must not be NULL and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_discard(
    publisher: *const rcl_publisher_t,
    token: *mut core::ffi::c_void,
) -> rcl_ret_t {
    validate_not_null!(publisher, token);
    let publisher = &*publisher;
    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );
    // SAFETY: `token` is the backend token a prior `rclc_publisher_loan`
    // handed out on this publisher. `discard_raw` fires the backend's
    // pub_discard (or reclaims the arena staging buffer) — issue 0812
    // retired the per-loan Box this used to reconstitute.
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    match pub_handle.discard_raw(token as *mut _) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// The QoS profile this publisher is ACTUALLY running — issue 1437.
///
/// `rcl_publisher_get_actual_qos`. What the BACKEND GRANTED, which differs
/// from what `rclc_publisher_init_with_qos` was handed whenever the backend
/// serves something else (zenoh clamps history depth to its receive ring;
/// DDS negotiates; a launch-lowered `rclc_node_set_qos_overrides` row has
/// already been folded in before either). That difference is what answers
/// "why is nothing arriving": a RELIABLE reader does not match a BEST_EFFORT
/// writer, and a clamped depth drops samples the caller believed were kept.
///
/// **Per policy, and a policy the backend cannot report is an ABSENCE**, not
/// your request echoed back: it reads `NROS_QOS_*_UNKNOWN`. A backend with no
/// read-back at all (XRCE) answers UNKNOWN in every field. Compare against
/// your request field by field; do NOT assume agreement where you find it
/// without checking for the sentinel first.
///
/// Free: the profile was read once at create and retained, so this never
/// re-enters the transport and is safe from a callback.
///
/// # Parameters
/// * `publisher` - an initialized publisher
/// * `out_qos` - written on success; untouched on every error
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the publisher is not initialized
///
/// # Shape, and why it is not upstream's
/// `rcl` returns `const rmw_qos_profile_t *` — an interior pointer into the
/// handle. That costs a retained `rclc_qos_t` in every `rcl_publisher_t`,
/// i.e. RAM in every image including those that never ask, and it hands out a
/// pointer whose lifetime the caller has to reason about with no ownership
/// types to help. An out-parameter plus a status is this API's own shape
/// (RFC-0018), and it is the shape that lets the NOT_INIT case be reported
/// rather than encoded as NULL.
///
/// # Safety
/// * `publisher` must be a valid pointer to an initialized publisher.
/// * `out_qos` must be a valid, writable `rclc_qos_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_get_actual_qos(
    publisher: *const rcl_publisher_t,
    out_qos: *mut rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(publisher, out_qos);
    let publisher = &*publisher;
    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );
    let pub_handle = &*(publisher._opaque.as_ptr() as *const nros::internals::RmwPublisher);
    *out_qos = rclc_qos_t::from_qos_settings(nros_rmw::Publisher::actual_qos(pub_handle));
    RCL_RET_OK
}

/// Finalize a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to an initialized publisher
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if publisher is NULL
/// * `RCL_RET_NOT_INIT` if not initialized
/// * `RCLC_RET_STALE_NODE` if the node this publisher was created on has
///   already been finalised — the teardown order was wrong, and nothing is
///   dropped. Undocumented until issue 1386, which is the opposite defect to
///   the one it was filed for: three sites DOCUMENTED a verdict they could not
///   produce while this one produced a verdict it did not document.
///
/// # Safety
/// * `publisher` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_publisher_fini(
    publisher: *mut rcl_publisher_t,
    node: *mut crate::node::rcl_node_t,
) -> rcl_ret_t {
    // phase-492 W1.b — rcl's arity: the node the entity was created on.
    // Checked non-NULL only; teardown needs nothing from it (RFC-0089
    // ADOPT-BOUNDED, ledger `c:publisher_fini`).
    validate_not_null!(publisher, node);

    let publisher = &mut *publisher;

    validate_state!(
        publisher,
        rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    );

    // Drop the inline RMW publisher handle
    // phase-379 W4 — the node this entity was created on is gone, so its
    // teardown ORDER was wrong. Reported rather than performed: before W4 the
    // entity held a pointer nothing dereferenced and this case succeeded
    // silently, which is what made the ordering obligation unenforceable.
    if publisher.node.is_bound() && !crate::node::node_ref_is_live(publisher.node) {
        return crate::error::RCLC_RET_STALE_NODE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        core::ptr::drop_in_place(
            publisher._opaque.as_mut_ptr() as *mut nros::internals::RmwPublisher
        );
        // Issue 1972 — the backend registration died with the handle.
        crate::event::status_event_slots::release_owner(publisher._opaque.as_ptr() as usize);
    }

    publisher._opaque = [0u64; PUBLISHER_OPAQUE_U64S];
    publisher.node = crate::node::rclc_node_ref_t::none();
    publisher.state = rclc_publisher_state_t::RCLC_PUBLISHER_STATE_SHUTDOWN;

    RCL_RET_OK
}

impl rcl_publisher_t {
    /// Can this handle be USED? — the one predicate behind both
    /// `rcl_publisher_is_valid` and `rcl_publisher_get_topic_name`.
    ///
    /// phase-417 stage 3. The publisher is the one member of the `is_valid`
    /// family whose ANSWER did not change: `rclc_publisher_state_t` has no
    /// third live value, so `INITIALIZED` really is the whole of "usable"
    /// here. It carries the helper anyway so the class has ONE spelling and a
    /// future state has one place to be added — the sibling entities each grew
    /// a `POLLING` (and the client a `REGISTERED`) that their predicates were
    /// never taught about.
    pub(crate) const fn is_usable(&self) -> bool {
        matches!(
            self.state,
            rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
        )
    }
}

/// Get the topic name of a publisher.
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if the handle is not
///   usable (see `rcl_publisher_t::is_usable`) or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_publisher_get_topic_name(
    publisher: *const rcl_publisher_t,
) -> *const c_char {
    if publisher.is_null() {
        return ptr::null();
    }

    let publisher = &*publisher;
    if !publisher.is_usable() {
        return ptr::null();
    }

    publisher.topic_name.as_ptr() as *const c_char
}

/// How many subscriptions are on this publisher's topic, right now.
///
/// rcl's `rcl_publisher_get_subscription_count` with an executor parameter, hence the
/// distinct `rclc_` name
/// (phase-492 W1.b) — phase-444, ledger row
/// `c:publisher_get_subscription_count`.
///
/// # A WEAKER QUESTION THAN UPSTREAM'S, stated here rather than discovered
///
/// Upstream counts the subscriptions MATCHED to THIS publisher — peers whose
/// QoS is compatible with it. This counts every subscription DISCOVERED on the
/// topic, matched or not, because that is what the graph can answer and a
/// backend with no QoS negotiation cannot tell the two apart at all.
///
/// Non-inverting, and in the safe direction: a topic nobody subscribes to
/// still reads `0`, so the use this exists for — skip the work when nobody is
/// listening — is answered exactly. An INCOMPATIBLE peer makes it read `1`
/// where upstream reads `0`, i.e. you do work nobody receives. Never the
/// reverse.
///
/// # The EXECUTOR parameter, which upstream does not take
///
/// We open ONE transport session per image and the executor owns it, so the
/// graph is the executor's to read — the same receiver difference as every
/// `rclc_executor_get_*` graph verb (phase-381 W4). The name was upstream's,
/// counting on the extra argument to make a porter notice; RFC-0089's
/// 2026-10-12 rule 2 forbids an upstream name over a different arity, so it is
/// `rclc_` (phase-492 W1.b) and a ported call fails on the name.
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
/// * `executor` - The executor whose session sees the graph
/// * `out_count` - Receives the count
///
/// # Returns
/// * `RCL_RET_OK` — `*out_count` written.
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL pointer.
/// * `RCL_RET_NOT_INIT` — publisher or executor not usable.
/// * `RCL_RET_UNSUPPORTED` — the backend cannot read the graph (XRCE). A
///   DIFFERENT answer from `0`, and `*out_count` is not written.
///
/// # Safety
/// * `publisher` and `executor` must be valid pointers or NULL
/// * `out_count` must point to a writable `size_t` or be NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_get_subscription_count(
    publisher: *const rcl_publisher_t,
    executor: *mut crate::executor::rclc_executor_t,
    out_count: *mut usize,
) -> rcl_ret_t {
    if publisher.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let publisher = &*publisher;
    if !publisher.is_usable() {
        return RCL_RET_NOT_INIT;
    }
    crate::executor::rclc_executor_count_subscribers(
        executor,
        publisher.topic_name.as_ptr() as *const c_char,
        out_count,
    )
}

/// Is this publisher handle usable?
///
/// rcl's `rcl_publisher_is_valid`, whose contract is "true for any handle that
/// can be used" — the ported idiom is a guard. See
/// `rcl_publisher_t::is_usable` for which states those are.
///
/// # Parameters
/// * `publisher` - Pointer to a publisher
///
/// # Returns
/// * `true` if usable, `false` if finalised, uninitialised or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_publisher_is_valid(publisher: *const rcl_publisher_t) -> bool {
    if publisher.is_null() {
        return false;
    }

    (*publisher).is_usable()
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
        let type_info = rclc_message_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
            serialized_size_max: 4,
            serialize: None,
            deserialize: None,
        };

        let mut node = crate::node::rcl_get_zero_initialized_node();

        // NULL publisher → INVALID_ARGUMENT
        assert_eq!(
            unsafe {
                rclc_publisher_init_default(
                    core::ptr::null_mut(),
                    &node,
                    &type_info,
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL node → INVALID_ARGUMENT
        let mut pub_ = rcl_get_zero_initialized_publisher();
        assert_eq!(
            unsafe {
                rclc_publisher_init_default(
                    &mut pub_,
                    core::ptr::null(),
                    &type_info,
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL type_info → INVALID_ARGUMENT
        let mut pub_ = rcl_get_zero_initialized_publisher();
        assert_eq!(
            unsafe {
                rclc_publisher_init_default(
                    &mut pub_,
                    &node,
                    core::ptr::null(),
                    topic.as_ptr() as *const core::ffi::c_char,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL topic → INVALID_ARGUMENT
        let mut pub_ = rcl_get_zero_initialized_publisher();
        assert_eq!(
            unsafe { rclc_publisher_init_default(&mut pub_, &node, &type_info, core::ptr::null()) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn publisher_zero_initialized_state() {
        let pub_ = rcl_get_zero_initialized_publisher();
        assert_eq!(
            pub_.state,
            rclc_publisher_state_t::RCLC_PUBLISHER_STATE_UNINITIALIZED,
        );
        assert!(!pub_.node.is_bound(), "fini must unbind the node reference");
        assert!(pub_._opaque.iter().all(|&v| v == 0));
    }
}

/// Name resolution on the eager entity-init paths (phase-428 W5 finding 3).
///
/// `rclc_publisher_init_with_qos` used to hand `TopicInfo` the RAW string the
/// caller passed. Every other C entity kind reaches
/// `Executor::resolve_entity_name` through `nros_executor_add_*`; a publisher
/// has no registration step, so it never did — and `.with_namespace()` does not
/// substitute, because the zenoh key is built from `TopicInfo::name` alone
/// (`nros-rmw-zenoh/src/keyexpr.rs::to_key`, whose own test
/// `an_unresolved_name_and_its_resolved_form_are_different_keys` pins that).
///
/// The resolution runs BEFORE the session is resolved, so it is observable
/// with no backend session at all: an unresolvable name answers
/// `RCL_RET_INVALID_ARGUMENT` where the un-fixed code got as far as
/// `RCL_RET_NOT_INIT`. (`mod publisher` itself is `rmw-cffi`-gated in
/// `lib.rs`, so these run under that feature.)
#[cfg(test)]
mod name_resolution_tests {
    use super::*;
    use crate::node::resolve_entity_name_on_node;

    /// A legacy (`rclc_node_init_default`) node with a name and namespace.
    /// Not executor-bound, so `resolve_entity_name_on_node` takes its
    /// expansion-only arm — the arm every standalone C example is on.
    fn node_named(name: &str, namespace: &str) -> rcl_node_t {
        let mut node = crate::node::rcl_get_zero_initialized_node();
        node.name[..name.len()].copy_from_slice(name.as_bytes());
        node.name_len = name.len();
        node.namespace[..namespace.len()].copy_from_slice(namespace.as_bytes());
        node.namespace_len = namespace.len();
        node.state = rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED;
        node
    }

    /// THE regression: on a `/ns` node, a publisher and a subscription created
    /// with the same string must reach the same wire name.
    ///
    /// Both call sites go through one function, and the assertion that it is
    /// the RIGHT function is the third one: `names::resolve_name` is what
    /// `Executor::resolve_entity_name_for` — the L2/registration path — calls,
    /// so publisher, polling subscription and callback subscription cannot
    /// drift.
    ///
    /// Fails on the pre-fix code, where the publisher's name was the raw
    /// `"chatter"` while the subscription's was `"/ns/chatter"`.
    #[test]
    fn namespaced_publisher_and_subscription_reach_the_same_name() {
        let node = node_named("talker", "/ns");

        // The call `rclc_publisher_init_with_qos` makes.
        let publisher_side = unsafe { resolve_entity_name_on_node(&node, "chatter") }
            .expect("a relative name on a named node must resolve");
        // The call `rclc_polling_subscription_init_with_qos` makes.
        let subscription_side = unsafe { resolve_entity_name_on_node(&node, "chatter") }
            .expect("a relative name on a named node must resolve");
        // What the executor-registration path (`nros_executor_add_subscription_*`
        // → `Executor::resolve_entity_name_for`) computes for the same input.
        let registration_side =
            nros_node::names::resolve_name("chatter", "talker", "/ns", core::iter::empty())
                .expect("the registration seam must resolve it too");

        assert_eq!(publisher_side.as_str(), "/ns/chatter");
        assert_eq!(publisher_side, subscription_side);
        assert_eq!(publisher_side, registration_side);
        // The pre-fix value, stated so the test says what it is protecting.
        assert_ne!(publisher_side.as_str(), "chatter");
    }

    /// `~` is the case a namespace alone could never have covered: it needs the
    /// NODE NAME too, which `TopicInfo` never carried into the key.
    #[test]
    fn a_private_name_expands_against_the_node_not_just_the_namespace() {
        let node = node_named("filter", "/sensing");
        let resolved = unsafe { resolve_entity_name_on_node(&node, "~/points") }
            .expect("a private name on a named node must resolve");
        assert_eq!(resolved.as_str(), "/sensing/filter/points");
    }

    /// An absolute name is already the wire name — resolution must not touch
    /// it. This is why the fix is invisible to every root-namespace C example.
    #[test]
    fn an_absolute_name_passes_through_unchanged() {
        let node = node_named("talker", "/ns");
        let resolved = unsafe { resolve_entity_name_on_node(&node, "/chatter") }
            .expect("an absolute name must resolve");
        assert_eq!(resolved.as_str(), "/chatter");
    }

    /// The resolver is WIRED INTO the FFI entry point, not merely present.
    ///
    /// Observable with no session: the resolution runs before
    /// `resolve_session_and_domain`, so a name that cannot be expanded is
    /// `RCL_RET_INVALID_ARGUMENT` here. `~` needs a node name; this node has
    /// none, so `expand_name` errors. Pre-fix this call never consulted the
    /// name at all and reached the session lookup, answering
    /// `RCL_RET_NOT_INIT`.
    #[test]
    fn an_unresolvable_name_is_rejected_by_publisher_init() {
        let node = node_named("", "/ns");
        let type_name = b"std_msgs::msg::dds_::Int32_\0";
        let type_hash = b"RIHS01_test\0";
        let type_info = rclc_message_type_t {
            type_name: type_name.as_ptr() as *const c_char,
            type_hash: type_hash.as_ptr() as *const c_char,
            serialized_size_max: 4,
            serialize: None,
            deserialize: None,
        };
        let topic = b"~/private\0";
        let mut pub_ = rcl_get_zero_initialized_publisher();

        let ret = unsafe {
            rclc_publisher_init_default(
                &mut pub_,
                &node,
                &type_info,
                topic.as_ptr() as *const c_char,
            )
        };
        assert_eq!(
            ret, RCL_RET_INVALID_ARGUMENT,
            "a name the resolver cannot expand must be refused, never published raw"
        );
    }
}

/// issue 1386 — the arm that STAYS, and the proof that it fires.
///
/// The generation mechanism (phase-379 W4) has one legitimate shape: a
/// reference minted when the entity was created, compared LATER against the
/// slot's current generation. Three call sites minted and compared in the same
/// expression, which is constant true, and those are gone. This is the shape
/// that is not — and `RCLC_RET_STALE_NODE` has to stay reachable from
/// somewhere, or the code would be the second half of the same defect.
#[cfg(test)]
mod stale_node_tests {
    use super::*;

    /// Finalising a publisher AFTER its node is a reported error, not a silent
    /// success. Before phase-379 W4 the entity held a `*const rcl_node_t` that
    /// nothing dereferenced, so the caller's teardown-order obligation was
    /// unenforceable.
    #[test]
    fn publisher_fini_after_its_node_reports_a_stale_node() {
        // Slot 1 is untouched by the other generation tests in this binary
        // (`node.rs` uses 2, 3 and MAX_NODES-1); `NODE_GENERATIONS` is one
        // process-wide static.
        let mut node = crate::node::rcl_get_zero_initialized_node();
        node.state = rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED;
        node.node_id = 1;

        let mut publisher = rcl_get_zero_initialized_publisher();
        publisher.state = rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED;
        // What `rclc_publisher_init_default` records at creation.
        publisher.node = unsafe { crate::node::node_ref_of(&node) };
        assert!(
            publisher.node.is_bound(),
            "precondition: the entity recorded a binding"
        );

        // The caller gets the order wrong: node first.
        assert_eq!(unsafe { crate::node::rcl_node_fini(&mut node) }, RCL_RET_OK);

        assert_eq!(
            unsafe { rcl_publisher_fini(&mut publisher, &mut node) },
            RCLC_RET_STALE_NODE,
            "a publisher outliving its node must SAY so — this is the one \
             verdict the generation scheme exists to produce"
        );
        assert_eq!(
            publisher.state,
            rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED,
            "a refused fini must not half-finalise the handle"
        );
    }
}
