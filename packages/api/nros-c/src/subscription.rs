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
    node::{rcl_node_t, rclc_node_state_t},
    opaque_sizes::SUBSCRIPTION_OPAQUE_U64S,
    publisher::rclc_message_type_t,
    qos::rclc_qos_t,
};

/// Subscription callback function type.
///
/// # Parameters
/// * `data` - Pointer to received CDR-serialized message data
/// * `len` - Length of data in bytes
/// * `context` - User-provided context pointer
pub type rclc_subscription_callback_t =
    Option<unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void)>;

/// Subscription callback that also receives the sample's wire **attachment**
/// (Phase 189.M3.4 — the C analog of the Rust
/// `node.subscription(t).generic(..).message_info()` builder path). Used by
/// `rclc_executor_add_subscription_raw_with_info`.
///
/// # Parameters
/// * `data` / `len` — received CDR bytes.
/// * `attachment` / `attachment_len` — the sample's wire attachment
///   (`attachment_len == 0` ⇒ none). Cross-RMW bridges read the
///   `bridge_origin` tag from it. Valid only during the call.
/// * `context` — user-provided context pointer.
pub type rclc_subscription_info_callback_t = Option<
    unsafe extern "C" fn(
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
        context: *mut c_void,
    ),
>;

/// Subscription state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_subscription_state_t {
    /// Not initialized
    RCLC_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /// Initialized. For a `rcl_subscription_t` the subscriber entity is
    /// created later, by `nros_executor_add_subscription_*`, inside the
    /// executor arena. For a `rclc_polling_subscription_t` the subscriber is
    /// live in the struct's own storage.
    RCLC_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /// Shutdown
    RCLC_SUBSCRIPTION_STATE_SHUTDOWN = 2,
}

/// A subscription the executor dispatches (rclc's `rcl_subscription_t`).
///
/// phase-487 W1 (RFC-0096 D9, the C amendment): an entity the executor
/// dispatches is owned by the executor. The subscriber, its receive buffer,
/// the callback and its context all live in the executor arena's entry, and
/// this struct holds the arena handle `(handle_id, _executor)` plus the
/// DESCRIPTION the registration needs. The description has to be here
/// because rclc's order is init first, `rclc_executor_add_*` second: at init
/// there is no executor to hold it yet.
///
/// Before phase-487 this struct also carried the callback, the context and
/// the polled form's subscriber storage (`_opaque`). On the dispatched road
/// none of the three was read after registration, because the arena keeps its
/// own copies. A subscription the caller drains itself is a
/// [`rclc_polling_subscription_t`].
#[repr(C)]
pub struct rcl_subscription_t {
    /// Current state
    pub state: rclc_subscription_state_t,
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
    /// phase-492 W8 — the descriptor this subscription was created with, kept
    /// so `rclc_executor_add_subscription_with_context` deserialises through
    /// its codec with rclc's arguments. Descriptors are generated
    /// `static const`s, so the pointer outlives the subscription.
    pub type_support: *const rclc_message_type_t,
    /// QoS settings (stored during init, used by executor registration)
    pub qos: crate::qos::rclc_qos_t,
    /// Handle ID from executor registration (SIZE_MAX = not registered)
    pub handle_id: usize,
    /// Phase 189.M3 — scheduling-context slot requested via
    /// [`rclc_subscription_init_with_options`]. `0` = inherit the
    /// executor / Node default (no explicit bind), matching the
    /// `rcl_node_options_t::sched_context_id` convention. When non-zero,
    /// `nros_executor_add_subscription_*` binds the freshly-created
    /// handle to this SC after registration.
    pub sched_context_id: crate::executor::rclc_sched_context_id_t,
    /// issue 1437 — the `rclc_executor_t` holding this subscription's arena
    /// entry, or NULL before registration. Set beside
    /// [`handle_id`](Self::handle_id) by `nros_executor_add_subscription_*`:
    /// the pair is the same `(executor_ptr, arena_entry_index)` that
    /// `ServiceServerInternal` and `ServiceClientInternal` carry.
    ///
    /// Internal: treat as opaque.
    pub _executor: *mut c_void,
}

impl Default for rcl_subscription_t {
    fn default() -> Self {
        Self {
            state: rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_UNINITIALIZED,
            topic_name: [0u8; MAX_TOPIC_LEN],
            topic_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            node: crate::node::rclc_node_ref_t::none(),
            type_support: ptr::null(),
            qos: crate::qos::rclc_qos_t::default(),
            handle_id: usize::MAX,
            sched_context_id: 0,
            _executor: ptr::null_mut(),
        }
    }
}

/// A subscription the CALLER drains, with no executor (phase-487 W1).
///
/// The C counterpart of C++'s `nros::PollSubscription<M>` and Rust's
/// `create_polling_subscription`. Nothing dispatches it, so it has no arena
/// slot and keeps its subscriber in its own storage: `_opaque` holds a
/// `RawSubscription<MESSAGE_BUFFER_SIZE>` (subscriber, receive buffer and
/// event registrations) from `rclc_polling_subscription_init*` until
/// `rclc_polling_subscription_fini`. Zero-initialised is UNINITIALIZED.
///
/// It was the `NROS_SUBSCRIPTION_STATE_POLLING` state of
/// `rcl_subscription_t` until phase-487, which made every dispatched
/// subscription carry this storage unused.
#[repr(C)]
pub struct rclc_polling_subscription_t {
    /// Current state: UNINITIALIZED, INITIALIZED (subscriber live) or SHUTDOWN.
    pub state: rclc_subscription_state_t,
    /// The node the subscriber was created on, kept so `fini` can refuse a
    /// teardown after the node's (`RCLC_RET_STALE_NODE`).
    pub node: crate::node::rclc_node_ref_t,
    /// phase-492 W8 — the descriptor this subscription was created with, kept
    /// so [`rclc_polling_subscription_take`] deserialises through its codec.
    pub type_support: *const rclc_message_type_t,
    /// Inline storage for the `RawSubscription`. Internal: treat as opaque.
    pub _opaque: [u64; SUBSCRIPTION_OPAQUE_U64S],
}

impl Default for rclc_polling_subscription_t {
    fn default() -> Self {
        Self {
            state: rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_UNINITIALIZED,
            node: crate::node::rclc_node_ref_t::none(),
            type_support: ptr::null(),
            _opaque: [0u64; SUBSCRIPTION_OPAQUE_U64S],
        }
    }
}

impl rclc_polling_subscription_t {
    /// The live `RawSubscription`, or `None` if this subscription is not
    /// INITIALIZED.
    #[cfg(feature = "rmw-cffi")]
    unsafe fn raw(
        &mut self,
    ) -> Option<&mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>> {
        if self.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
            return None;
        }
        Some(unsafe {
            &mut *(self._opaque.as_mut_ptr()
                as *mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>)
        })
    }
}

/// Get a zero-initialized polling subscription.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_get_zero_initialized_polling_subscription() -> rclc_polling_subscription_t {
    rclc_polling_subscription_t::default()
}

/// Get a zero-initialized subscription.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_zero_initialized_subscription() -> rcl_subscription_t {
    rcl_subscription_t::default()
}

/// Phase 189.M3 — rclc-style named subscription options.
///
/// Sits ALONGSIDE the QoS profile (rclc convention): QoS is passed
/// separately, this struct carries the non-QoS subscription-creation
/// axes. The struct holds only plain scalar fields — no pointers — so it
/// is safe to stack-allocate, memcpy, and pass across the FFI. Zero-init
/// (all fields 0) selects the default behaviour, identical to
/// `rclc_subscription_init_with_qos`.
#[repr(C)]
#[derive(Default)]
pub struct rcl_subscription_options_t {
    /// Scheduling-context slot to bind the subscription's executor
    /// handle to. `0` = inherit the executor / Node default (no explicit
    /// bind), matching the `rcl_node_options_t::sched_context_id`
    /// convention. A non-zero value must be an id previously returned
    /// from `rclc_executor_create_sched_context`; the bind is applied by
    /// `nros_executor_add_subscription_*` once the handle exists. Has
    /// no effect on the L1 polling path.
    pub sched_context: crate::executor::rclc_sched_context_id_t,
    /// Reserved — needs a with-info arena path (Phase 189.M3.4), not yet
    /// wired. When a future M3.4 lands this will request the
    /// message-info delivery variant (sample identity + reception
    /// timestamp alongside the payload). Setting it to a non-zero value
    /// today is accepted but ignored. Treat as bool (0 = off).
    pub message_info: u8,
    /// Reserved for future use; must be zero. Pads the struct for ABI
    /// stability so later axes can be added without a layout break.
    pub _reserved: [u8; 2],
}

/// Get a zero-initialised [`rcl_subscription_options_t`].
///
/// All fields default to "inherit"/"off": `sched_context = 0` (executor
/// default), `message_info = 0` (reserved, off). Callers populate only
/// the fields they want before passing the struct to
/// [`rclc_subscription_init_with_options`].
#[unsafe(no_mangle)]
pub extern "C" fn rcl_subscription_get_default_options() -> rcl_subscription_options_t {
    rcl_subscription_options_t::default()
}

/// Initialize a subscription with default QoS (RELIABLE, KEEP_LAST(10)).
///
/// rclc's `rclc_subscription_init_default`, in rclc's argument order and at
/// rclc's ARITY (phase-417 stage 6). The old `nros_subscription_init` carried
/// two extra arguments — `callback` and `context` — and RFC-0089 records why
/// they are gone from here:
///
/// * the binding site was never mandated. RFC-0041 is about the DISPATCH
///   MODEL, not about which call first supplies a callback, and
///   `executor-owns-no-entity-storage` — cited by name in ten ledger rows as
///   the reason — is defined nowhere in `docs/design/`;
/// * the only real constraint found (`c:timer_exchange_callback`) forbids
///   SWAPPING a callback on a live entity, which registration-time binding
///   does not do.
///
/// So the callback is supplied where rclc supplies it, at registration:
/// `rclc_executor_add_subscription_with_context` for the typed path (rclc's
/// own, phase-492 W8) and `rclc_executor_add_subscription_raw` for the byte
/// path, which has no rclc counterpart and therefore takes an ours-only name.
///
/// The typesupport parameter stays `const rclc_message_type_t *` rather than
/// taking `rosidl_message_type_support_t`'s name: a rosidl typesupport's
/// MEMBERS are its contract, including the `func` dispatcher we do not have,
/// so adopting the name would claim a structure we lack (RFC-0089, settled
/// 2026-09-04). It costs a ported call site nothing — the argument comes from
/// our codegen either way.
///
/// The six-argument `nros_subscription_init` is GONE (phase-417 stage 6 step
/// B retired it), and since phase-487 no init entry point takes a callback:
/// the subscription struct no longer has anywhere to keep one, because the
/// executor arena owns it.
///
/// # Parameters
/// * `subscription` - Pointer to a zero-initialized subscription
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
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
pub unsafe extern "C" fn rclc_subscription_init_default(
    subscription: *mut rcl_subscription_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
) -> rcl_ret_t {
    rclc_subscription_init_with_qos(subscription, node, type_info, topic_name, ptr::null())
}

/// Initialize a subscription with custom QoS.
///
/// rclc's `rclc_subscription_init` shape: `(subscription, node, type, topic,
/// qos)`. phase-487 W1 removed the `callback` and `context` this took
/// between the topic and the QoS. The struct stored them only for
/// the (since removed) three-argument `nros_executor_add_subscription` to read back, and the arena kept its own
/// copies; the callback is now supplied at registration, by
/// `rclc_executor_add_subscription_raw` / `_typed` / `_with_info`.
///
/// # Safety
/// * All required pointers must be valid
/// * `topic_name` must be a valid null-terminated string
/// * `qos` may be NULL (default) or point to a valid `rclc_qos_t`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_init_with_qos(
    subscription: *mut rcl_subscription_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
    qos: *const rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(subscription, node, type_info, topic_name);

    let subscription = &mut *subscription;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        subscription,
        rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );

    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // Copy topic name (required — empty rejected)
    subscription.topic_name_len =
        crate::util::copy_cstr_into(topic_name, &mut subscription.topic_name);
    if subscription.topic_name_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    subscription.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut subscription.type_name);
    subscription.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut subscription.type_hash);

    subscription.node = unsafe { crate::node::node_ref_of(node) };
    // phase-492 W8 — the codec the typed registration reads.
    subscription.type_support = type_info;

    // Store QoS settings for later use by executor registration
    subscription.qos = if qos.is_null() {
        crate::qos::RCLC_QOS_DEFAULT
    } else {
        *qos
    };

    // Subscriber creation is deferred to `nros_executor_add_subscription_*`,
    // which creates it inside the executor arena.
    subscription.handle_id = usize::MAX;
    // issue 1437 — the index and the arena it indexes are ONE fact;
    // clearing half would leave a stale executor pointer paired with
    // "not registered".
    subscription._executor = ptr::null_mut();
    subscription.state = rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED;

    RCL_RET_OK
}

/// Phase 189.M3 — initialize a subscription with custom QoS + named options.
///
/// rclc-style entry point: QoS is passed separately (`qos`, NULL =
/// default) and the non-QoS axes ride in `options` (NULL = defaults).
/// Behaves exactly like [`rclc_subscription_init_with_qos`] except that
/// a non-zero `options->sched_context` is stashed on the subscription so
/// that `nros_executor_add_subscription_*` binds the resulting
/// executor handle to that scheduling context once the handle is known
/// (entity creation is deferred to registration, so the handle does not
/// exist at init time). `options->message_info` is RESERVED and ignored
/// (Phase 189.M3.4).
///
/// # Parameters
/// * `subscription` - Pointer to a zero-initialized subscription
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated string)
/// * `qos` - Pointer to QoS settings (NULL for default)
/// * `options` - Pointer to subscription options (NULL for defaults)
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
pub unsafe extern "C" fn rclc_subscription_init_with_options(
    subscription: *mut rcl_subscription_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
    qos: *const rclc_qos_t,
    options: *const rcl_subscription_options_t,
) -> rcl_ret_t {
    let ret = rclc_subscription_init_with_qos(subscription, node, type_info, topic_name, qos);
    if ret != RCL_RET_OK {
        return ret;
    }

    // Stash the requested scheduling-context slot. The handle does not
    // exist yet (entity creation + handle assignment happen in
    // `nros_executor_add_subscription_*`), so the actual
    // `bind_handle_to_sched_context` call is deferred to there. `0`
    // means "inherit the default" and is a no-op. `message_info` is
    // reserved (Phase 189.M3.4) and intentionally ignored.
    if !options.is_null() {
        let opts = &*options;
        // SAFETY: `subscription` was validated non-NULL by the
        // `_with_qos` call above (it returned OK).
        (*subscription).sched_context_id = opts.sched_context;
    }

    RCL_RET_OK
}

// ============================================================================
// Phase 122.3.b — Layer-1 primitive entry points (caller polls)
// ============================================================================
//
// L1 is for callers that own their own scheduler (RTIC, embassy,
// FreeRTOS-native task-per-entity). The subscriber entity is created
// at init time and stored inline in `_opaque`; the caller polls it
// directly via `rclc_polling_subscription_take_serialized`, never
// registering with an `rclc_executor_t`.
//
// phase-487 W1 — L1 is its own TYPE, `rclc_polling_subscription_t`, so
// "a polled subscription cannot be added to an executor" is a compile
// error rather than a state check, and a dispatched `rcl_subscription_t`
// no longer carries the storage only this road fills. State transitions:
//
//   UNINITIALIZED → INITIALIZED (via rclc_polling_subscription_init*)
//   INITIALIZED   → SHUTDOWN    (via rclc_polling_subscription_fini)

/// Phase 122.3.b — initialize an L1 polling-mode subscription.
///
/// Creates the underlying RMW subscriber immediately and stores it
/// inline in the subscription's `_opaque` field. The caller drains
/// received messages via `rclc_polling_subscription_take_serialized`.
///
/// Uses default QoS (RELIABLE, KEEP_LAST(10)). For custom QoS, use
/// `rclc_polling_subscription_init_with_qos`.
///
/// # Parameters
/// * `subscription` - Pointer to a zero-initialized subscription
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to message type information
/// * `topic_name` - Topic name (null-terminated)
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any pointer is NULL or topic empty
/// * `RCL_RET_NOT_INIT` if node / support not initialized
/// * `RCL_RET_ERROR` if subscriber creation failed
///
/// # Safety
/// * All pointers must be valid
/// * `topic_name` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_init(
    subscription: *mut rclc_polling_subscription_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
) -> rcl_ret_t {
    rclc_polling_subscription_init_with_qos(subscription, node, type_info, topic_name, ptr::null())
}

/// Phase 122.3.b — initialize an L1 polling-mode subscription with custom QoS.
///
/// See `rclc_polling_subscription_init` for the threading + lifecycle
/// contract.
///
/// # Safety
/// See `rclc_polling_subscription_init`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_init_with_qos(
    subscription: *mut rclc_polling_subscription_t,
    node: *const rcl_node_t,
    type_info: *const rclc_message_type_t,
    topic_name: *const c_char,
    qos: *const rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(subscription, node, type_info, topic_name);

    let subscription_mut = &mut *subscription;
    let node_ref = &*node;
    let type_info_ref = &*type_info;

    validate_state!(
        subscription_mut,
        rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // The description is needed only to create the subscriber, which
    // happens below, so it is read into locals: the polled struct keeps the
    // subscriber, not the names it was created from. Same truncation as the
    // dispatched road's `copy_cstr_into` into its struct.
    let mut topic_buf = [0u8; MAX_TOPIC_LEN];
    let topic_len = crate::util::copy_cstr_into(topic_name, &mut topic_buf);
    if topic_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let mut type_buf = [0u8; MAX_TYPE_NAME_LEN];
    let type_len = crate::util::copy_cstr_into(type_info_ref.type_name, &mut type_buf);
    let mut hash_buf = [0u8; MAX_TYPE_HASH_LEN];
    let hash_len = crate::util::copy_cstr_into(type_info_ref.type_hash, &mut hash_buf);
    let qos_value = if qos.is_null() {
        crate::qos::RCLC_QOS_DEFAULT
    } else {
        *qos
    };
    // Read only by the subscriber creation below, which a no-RMW build omits.
    #[cfg(not(feature = "rmw-cffi"))]
    let _ = (&type_buf, type_len, &hash_buf, hash_len, &qos_value);

    // Resolve the SOURCE topic name to its wire name. The L2 (callback) path
    // gets this from `nros_executor_add_subscription_*`; the L1 path creates the
    // entity right here and so never reached that seam — the same gap
    // `rclc_publisher_init_with_qos` had, and the reason both are fixed
    // together rather than at the site the sweep reported.
    let _resolved_topic = {
        let source = core::str::from_utf8_unchecked(&topic_buf[..topic_len]);
        match crate::node::resolve_entity_name_on_node(node_ref, source) {
            Some(r) => r,
            None => return RCL_RET_INVALID_ARGUMENT,
        }
    };

    // Create the subscriber NOW (vs deferred for L2). The L1 path
    // owns the entity inline; no executor arena involved.
    #[cfg(feature = "rmw-cffi")]
    {
        use nros_node::{Session, TopicInfo};

        // Phase 156 Sub-bug D — multi-Session dispatch (see
        // `rclc_publisher_init_default` for the long form).
        let (session, domain_id) = match crate::node::resolve_session_and_domain(node_ref) {
            Some(t) => t,
            None => return RCL_RET_NOT_INIT,
        };

        let topic_str = core::str::from_utf8_unchecked(&topic_buf[..topic_len]);
        let type_str = core::str::from_utf8_unchecked(&type_buf[..type_len]);
        let type_hash_str = core::str::from_utf8_unchecked(&hash_buf[..hash_len]);
        let node_name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
        let namespace_str =
            core::str::from_utf8_unchecked(&node_ref.namespace[..node_ref.namespace_len]);

        // RESOLVED name on the wire; `topic_str` stays the SOURCE spelling for
        // the QoS-override lookup below (the plan writes overrides against
        // launch names — same split as `nros-cpp/src/publisher.rs`).
        let topic_info = TopicInfo::new(_resolved_topic.as_str(), type_str, type_hash_str)
            .with_domain(domain_id)
            .with_node_name(node_name_str)
            .with_namespace(namespace_str);

        // Phase 211.H (issue #52) — fold any plan qos_overrides for this
        // topic + subscription role into the profile before create, mirroring
        // Rust's `NodeHandle::create_subscription_with_qos`.
        let qos_settings = crate::qos::apply_qos_overrides(
            qos_value.to_qos_settings(),
            node_ref.qos_overrides,
            node_ref.qos_overrides_len,
            topic_str,
            crate::qos::QOS_OVERRIDE_ROLE_SUBSCRIPTION,
        );
        // phase-454 W10 — the REGISTRATION half of C's declared-depth check.
        // `NROS_ASSERT_DECLARED_DEPTH` catches a C call site whose depth is a
        // constant expression at COMPILE time; a `rclc_qos_t` built at run time
        // has no constant to assert on, and this is where it lands instead.
        if nros_node::declared_qos::check(topic_info.type_name, topic_info.name, &qos_settings)
            .is_err()
        {
            return RCL_RET_ERROR;
        }
        match session.create_subscription(&topic_info, qos_settings) {
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
            Err(_) => return RCL_RET_ERROR,
        }
    }

    subscription_mut.node = unsafe { crate::node::node_ref_of(node) };
    subscription_mut.type_support = type_info;
    subscription_mut.state = rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED;
    RCL_RET_OK
}

/// Phase 122.3.c.6.e — register a C wake callback on an L1
/// polling-mode subscription. `state` is a caller-owned
/// `rclc_wake_state_t` (declared next to the subscription) that
/// must outlive the subscription and not move. Pass `cb = NULL`
/// to disable. The backend wakes the callback when a new message
/// arrives.
///
/// # Safety
/// All pointers valid; `state` storage stable for the
/// subscription's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_set_wake_callback(
    subscription: *mut rclc_polling_subscription_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    if subscription.is_null() || state.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCL_RET_INVALID_ARGUMENT;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = subscription_mut.raw() else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        let state_ptr = state as *mut nros_node::c_waker::CWakeState;
        core::ptr::write(
            state_ptr,
            nros_node::c_waker::CWakeState { fn_ptr: cb, ctx },
        );
        let waker = nros_node::c_waker::make_waker(state_ptr);
        raw.register_waker(&waker);
        RCL_RET_OK
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (state, cb, ctx);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.b — non-blocking poll on an L1 polling-mode
/// subscription. Returns the number of bytes received on success
/// (may be 0 if no data available), or a negative `rcl_ret_t` on
/// error.
///
/// # Parameters
/// * `subscription` - Pointer to an initialized polling subscription
/// * `buf` - Caller-supplied buffer to receive the message bytes
/// * `buf_len` - Capacity of `buf`
///
/// # Returns
/// * `>= 0` — number of bytes copied into `buf`
/// * `RCL_RET_INVALID_ARGUMENT` if any pointer is NULL or state is wrong
/// * `RCL_RET_ERROR` on transport failure
///
/// # Safety
/// * `subscription` must be an initialized polling subscription
/// * `buf` must point to writable memory of at least `buf_len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_take_serialized(
    subscription: *mut rclc_polling_subscription_t,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    if subscription.is_null() || (buf.is_null() && buf_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = subscription_mut.raw() else {
            return RCLC_RET_BAD_SEQUENCE;
        };
        match raw.take_serialized() {
            Ok(Some(len)) => {
                let to_copy = len.min(buf_len);
                core::ptr::copy_nonoverlapping(raw.buffer().as_ptr(), buf, to_copy);
                to_copy as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (subscription_mut, buf, buf_len);
        RCL_RET_ERROR
    }
}

/// upstream rmw's subscription pre-allocation handle, declared INCOMPLETE
/// (phase-492 W8): a pointer to one compiles, so NULL can be passed; declaring
/// one does not. nano-ros has no pre-allocation to hand out.
pub struct rmw_subscription_allocation_t {
    _private: [u8; 0],
}

/// Take one sample from a polling subscription and DESERIALISE it into
/// `ros_message` through the subscription's descriptor — the typed sibling of
/// [`rclc_polling_subscription_take_serialized`] (phase-492 W8).
///
/// This is the `rcl_take` analog. `rcl_take` itself is not declared: it takes
/// an `rcl_subscription_t`, which here is executor-owned (phase-487 W1, RFC-0096
/// D9) and has no subscriber of its own to take from, so the upstream name with
/// this contract would compile and differ. A subscription the caller drains is
/// an `rclc_polling_subscription_t`, and this is its take.
///
/// `ros_message` is the generated struct of the subscription's type, its strings
/// and sequences wired to caller memory (`<Msg>__init_in`). The sample is read
/// into the subscription's own receive buffer and decoded from there; nothing
/// allocates.
///
/// # Returns
/// * `RCL_RET_OK` — one sample was taken and decoded into `ros_message`
/// * `RCLC_RET_TRY_AGAIN` — nothing to take
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument, or a non-NULL `allocation`
/// * `RCLC_RET_BAD_SEQUENCE` — not initialised
/// * `RCL_RET_UNSUPPORTED` — the descriptor carries no deserialiser
/// * `RCL_RET_ERROR` — the transport failed, or the sample did not decode (a
///   field larger than its `capacity`): the sample is consumed and
///   `ros_message` is not a valid message
///
/// # Safety
/// * `subscription` must be an initialised polling subscription.
/// * `ros_message` must point at a message of the subscription's type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_take(
    subscription: *mut rclc_polling_subscription_t,
    ros_message: *mut c_void,
    allocation: *mut rmw_subscription_allocation_t,
) -> rcl_ret_t {
    if subscription.is_null() || ros_message.is_null() || !allocation.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }
    let Some(deserialize) = subscription_mut
        .type_support
        .as_ref()
        .and_then(|t| t.deserialize)
    else {
        return RCL_RET_UNSUPPORTED;
    };

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = subscription_mut.raw() else {
            return RCLC_RET_BAD_SEQUENCE;
        };
        match raw.take_serialized() {
            Ok(Some(len)) => {
                let bytes = raw.buffer();
                let len = len.min(bytes.len());
                if deserialize(bytes.as_ptr(), len, ros_message) == 0 {
                    RCL_RET_OK
                } else {
                    RCL_RET_ERROR
                }
            }
            Ok(None) => RCLC_RET_TRY_AGAIN,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (deserialize, ros_message);
        RCL_RET_ERROR
    }
}

/// Phase 252 / issue 0073 — E2E message-integrity status surfaced to the C/C++
/// receive path ([`rclc_polling_subscription_take_validated`]). The C analog of the
/// Rust `IntegrityStatus` / `CallbackCtx::integrity()`.
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub struct rclc_integrity_status_t {
    /// Sequence-number gap since the previous in-order message (0 = none).
    pub gap: i64,
    /// `true` if this sample's sequence number was already seen (a duplicate).
    pub duplicate: bool,
    /// CRC verdict: `1` = valid, `0` = mismatch (corruption), `-1` = no CRC on the
    /// wire (the publisher was built without `safety-e2e`).
    pub crc_valid: i8,
}

/// Phase 252 / issue 0073 — non-blocking poll that ALSO returns the E2E
/// integrity status (CRC + sequence gap/dup) of the received sample. The
/// safety-e2e analog of [`rclc_polling_subscription_take_serialized`]: it requests
/// validation on the backend (the zenoh shim recomputes + compares the CRC
/// attachment and tracks the sequence) and writes the verdict to `*out_status`.
///
/// Requires the build to enable `safety-e2e` on both ends (the zenoh backend's
/// own feature, lowered from a declared `[safety]` axis); otherwise `crc_valid`
/// reports `-1` (no CRC on the wire).
///
/// # Returns
/// * `>= 0` — number of bytes copied into `buf` (0 = no data available)
/// * `RCL_RET_INVALID_ARGUMENT` if `subscription`/`buf` is NULL
/// * `RCLC_RET_BAD_SEQUENCE` if the polling subscription is not initialized
/// * `RCL_RET_ERROR` on transport failure
///
/// # Safety
/// * `subscription` must be an initialized polling subscription
/// * `buf` must point to writable memory of at least `buf_len` bytes
/// * `out_status`, if non-NULL, must point to a writable `rclc_integrity_status_t`
#[cfg(feature = "safety-e2e")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_take_validated(
    subscription: *mut rclc_polling_subscription_t,
    buf: *mut u8,
    buf_len: usize,
    out_status: *mut rclc_integrity_status_t,
) -> i32 {
    if subscription.is_null() || (buf.is_null() && buf_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = subscription_mut.raw() else {
            return RCLC_RET_BAD_SEQUENCE;
        };
        match raw.take_validated() {
            Ok(Some((len, status))) => {
                let to_copy = len.min(buf_len);
                core::ptr::copy_nonoverlapping(raw.buffer().as_ptr(), buf, to_copy);
                if !out_status.is_null() {
                    *out_status = rclc_integrity_status_t {
                        gap: status.gap,
                        duplicate: status.duplicate,
                        crc_valid: match status.crc_valid {
                            Some(true) => 1,
                            Some(false) => 0,
                            None => -1,
                        },
                    };
                }
                to_copy as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (subscription_mut, buf, buf_len, out_status);
        RCL_RET_ERROR
    }
}

// ============================================================================
// Phase 124.A.6 — zero-copy subscription borrow / release
// ============================================================================

/// Phase 124.A.6 — borrow a read-only view of the next available message
/// in place (zero-copy receive path).
///
/// **Availability:** requires the `lending` AND `alloc` cargo features,
/// and no shipped build enables `lending` — see `rclc_publisher_loan`
/// for what that means for this header. Issue 0814.
///
/// Bypasses the subscription's staging buffer. On success, `*out_buf`
/// points at `*out_len` bytes the caller can read directly. Caller MUST
/// pass `*out_token` back to [`rclc_polling_subscription_release`] before
/// requesting another borrow on the same subscription — only one
/// outstanding view per subscription at a time.
///
/// Falls back to a `take_serialized` copy into the staging buffer when the
/// active backend's vtable doesn't expose a native borrow slot.
///
/// **Requires `alloc` as well as `lending`.** Unlike the publish half —
/// which issue 0812 made allocation-free by passing the BACKEND's own
/// token through — the receive half still boxes a `RecvView` per borrow
/// to manufacture a stable FFI token, so it cannot exist in a heap-free
/// image. That was 0812's own "what this does NOT yet buy" item, and
/// leaving the `cfg` at `lending` alone did not make it work: it made
/// `nros-c` FAIL TO COMPILE under `--features lending` without `alloc`
/// (four `alloc::boxed::Box` uses with no `extern crate alloc` in scope).
/// A capability that cannot build is worse than one that is absent, so
/// the `cfg` now states the real requirement (issue 0814).
///
/// # Returns
/// * `> 0` — message length written into `*out_len`; view is ready.
/// * `0` — no message ready right now.
/// * negative — error (see `rcl_ret_t`).
///
/// # Safety
/// * `subscription` must be a valid polling subscription.
/// * `out_buf` / `out_len` / `out_token` must be valid pointers.
#[cfg(all(feature = "lending", feature = "alloc"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_borrow(
    subscription: *mut rclc_polling_subscription_t,
    out_buf: *mut *const u8,
    out_len: *mut usize,
    out_token: *mut *mut core::ffi::c_void,
) -> i32 {
    if subscription.is_null() || out_buf.is_null() || out_len.is_null() || out_token.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }

    let Some(raw) = subscription_mut.raw() else {
        return RCLC_RET_BAD_SEQUENCE;
    };
    // RawSubscription::try_borrow returns a RecvView bound to &mut raw.
    // For the C-side token plumbing, we erase the lifetime and Box it
    // — the caller's release/destroy contract restores correctness.
    match raw.try_borrow() {
        Ok(Some(view)) => {
            // RecvView impls Deref<Target=[u8]> on both paths, so the
            // slice methods are reached by auto-deref (issue 0814: the
            // explicit `(&*view)` reborrow this replaces tripped clippy's
            // `needless_borrow` / `explicit_auto_deref` under `-D warnings`.
            // It had never been LINTED, because until now this file did not
            // compile under `lending` at all).
            let buf_ptr = view.as_ptr();
            let len = view.len();
            // SAFETY: erase the lifetime — caller must release before
            // dropping the subscription or requesting another borrow.
            let view_static: nros_node::RecvView<'static> = core::mem::transmute(view);
            let boxed = alloc::boxed::Box::new(view_static);
            *out_buf = buf_ptr;
            *out_len = len;
            *out_token = alloc::boxed::Box::into_raw(boxed) as *mut core::ffi::c_void;
            len as i32
        }
        Ok(None) => 0,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Phase 124.A.6 — release a previously borrowed view.
///
/// **Availability:** requires `lending` AND `alloc`, and no shipped build
/// enables `lending` — see `rclc_publisher_loan`. Issue 0814.
///
/// `token` MUST come from a prior `rclc_polling_subscription_borrow` on the
/// SAME subscription; consuming it is mandatory before the
/// subscription's next borrow / destroy.
///
/// **Requires `alloc` as well as `lending`** — it unboxes the `RecvView`
/// its paired `rclc_polling_subscription_borrow` boxed. See that function.
///
/// # Safety
/// * `subscription` must be the subscription the token was borrowed from.
/// * `token` must not be NULL and must not be reused after this call.
#[cfg(all(feature = "lending", feature = "alloc"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_release(
    subscription: *mut rclc_polling_subscription_t,
    token: *mut core::ffi::c_void,
) -> rcl_ret_t {
    if subscription.is_null() || token.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }
    // SAFETY: token came from `Box::into_raw(Box<RecvView<'static>>)`
    // inside `rclc_polling_subscription_borrow`. Reconstitute and drop —
    // RecvView::drop fires the backend's sub_release (or no-op for
    // the staging-buffer fallback path).
    let _view: alloc::boxed::Box<nros_node::RecvView<'static>> =
        alloc::boxed::Box::from_raw(token as *mut nros_node::RecvView<'static>);
    RCL_RET_OK
}

/// Phase 124.D.1 — burst-take. Drain up to `max_msgs` queued samples
/// into the contiguous `buf` block in a single call, with the i-th
/// sample at `buf + i * per_msg_cap` and length `out_lens[i]`.
///
/// `buf` is a caller-owned contiguous region of at least
/// `max_msgs * per_msg_cap` bytes. `out_lens` is a writable array
/// of at least `max_msgs` `size_t` slots.
///
/// Returns the number of messages delivered (`>= 0`) on success, or
/// a negative `rcl_ret_t` error code. Partial drains are reported
/// as the count, never as an error.
///
/// # Safety
/// * `subscription` must be an initialized polling subscription.
/// * `buf` must point to writable memory of at least
///   `max_msgs * per_msg_cap` bytes.
/// * `out_lens` must point to a writable array of at least
///   `max_msgs` `size_t` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_take_sequence(
    subscription: *mut rclc_polling_subscription_t,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
) -> i32 {
    if subscription.is_null() || buf.is_null() || out_lens.is_null() || per_msg_cap == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription_mut = &mut *subscription;
    if subscription_mut.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = subscription_mut.raw() else {
            return RCLC_RET_BAD_SEQUENCE;
        };
        // `RawSubscription`'s `handle` is the cffi subscriber that
        // drives the vtable slot (or its loop fallback). Borrow the
        // caller buffer as a flat slice and dispatch.
        let buf_slice = core::slice::from_raw_parts_mut(buf, max_msgs.saturating_mul(per_msg_cap));
        let lens_slice = core::slice::from_raw_parts_mut(out_lens, max_msgs);
        match raw.take_sequence(buf_slice, per_msg_cap, max_msgs, lens_slice) {
            Ok(count) => count as i32,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (subscription_mut, buf, per_msg_cap, max_msgs, out_lens);
        RCL_RET_ERROR
    }
}

/// The QoS profile this subscription is ACTUALLY running — issue 1437.
///
/// `rcl_subscription_get_actual_qos`. See
/// [`rclc_publisher_get_actual_qos`](crate::publisher::rclc_publisher_get_actual_qos)
/// for what "actual" means, why an unreportable policy reads back as
/// `NROS_QOS_*_UNKNOWN` rather than as your request, and why this API returns
/// a status with an out-parameter where `rcl` returns an interior pointer.
///
/// The executor arena owns this subscription's subscriber and the handle
/// carries only `(executor, handle_id)`, so the answer is read through the
/// arena. A polled subscription answers through
/// [`rclc_polling_subscription_get_actual_qos`].
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the subscription is not initialised, or has not
///   been added to an executor yet — there is no entity to ask, which is a
///   different answer from `UNKNOWN` ("there is an entity and it cannot say")
///
/// # Safety
/// * `subscription` must be a valid pointer to an initialized subscription.
/// * `out_qos` must be a valid, writable `rclc_qos_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_get_actual_qos(
    subscription: *const rcl_subscription_t,
    out_qos: *mut rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(subscription, out_qos);

    #[cfg(feature = "rmw-cffi")]
    {
        let subscription = &*subscription;
        let granted = match subscription.state {
            rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED => {
                if subscription._executor.is_null() || subscription.handle_id == usize::MAX {
                    return RCL_RET_NOT_INIT;
                }
                let exec_t =
                    &mut *(subscription._executor as *mut crate::executor::rclc_executor_t);
                let exec = crate::executor::get_executor(exec_t);
                match exec.subscription_handle(subscription.handle_id) {
                    Some(handle) => nros_rmw::Subscription::actual_qos(handle),
                    None => return RCL_RET_NOT_INIT,
                }
            }
            _ => return RCL_RET_NOT_INIT,
        };
        *out_qos = rclc_qos_t::from_qos_settings(granted);
        RCL_RET_OK
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = subscription;
        RCL_RET_NOT_INIT
    }
}

/// The QoS profile a polled subscription is ACTUALLY running.
///
/// The polled twin of [`rclc_subscription_get_actual_qos`]: the subscriber is
/// inline in the struct, so it answers directly.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the polling subscription is not initialised
///
/// # Safety
/// * `subscription` must be a valid pointer.
/// * `out_qos` must be a valid, writable `rclc_qos_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_get_actual_qos(
    subscription: *mut rclc_polling_subscription_t,
    out_qos: *mut rclc_qos_t,
) -> rcl_ret_t {
    validate_not_null!(subscription, out_qos);

    #[cfg(feature = "rmw-cffi")]
    {
        let Some(raw) = (*subscription).raw() else {
            return RCL_RET_NOT_INIT;
        };
        *out_qos = rclc_qos_t::from_qos_settings(raw.qos());
        RCL_RET_OK
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = subscription;
        RCL_RET_NOT_INIT
    }
}

/// Finalize a polled subscription: drops the inline subscriber, which closes
/// the RMW entity.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if `subscription` is NULL
/// * `RCL_RET_NOT_INIT` if it is not initialised
/// * `RCLC_RET_STALE_NODE` if its node has already been finalised — the
///   teardown order was wrong, and nothing is dropped (issue 1386).
///
/// # Safety
/// * `subscription` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_subscription_fini(
    subscription: *mut rclc_polling_subscription_t,
) -> rcl_ret_t {
    validate_not_null!(subscription);

    let subscription = &mut *subscription;
    if subscription.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }
    // phase-379 W4 — the node this entity was created on is gone, so its
    // teardown ORDER was wrong. Reported rather than performed: before W4 the
    // entity held a pointer nothing dereferenced and this case succeeded
    // silently, which is what made the ordering obligation unenforceable.
    if subscription.node.is_bound() && !crate::node::node_ref_is_live(subscription.node) {
        return crate::error::RCLC_RET_STALE_NODE;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        core::ptr::drop_in_place(subscription._opaque.as_mut_ptr()
            as *mut nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>);
        subscription._opaque = [0u64; SUBSCRIPTION_OPAQUE_U64S];
    }

    subscription.node = crate::node::rclc_node_ref_t::none();
    subscription.state = rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_SHUTDOWN;
    RCL_RET_OK
}

/// Finalize a subscription.
///
/// The subscriber is the executor arena's: remove it with
/// `rclc_executor_remove_subscription` first if it was added. This resets
/// the description and the handle.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if `subscription` is NULL
/// * `RCL_RET_NOT_INIT` if it is in no finalisable state
///   finalised — the teardown order was wrong, and nothing is dropped. Stated
///   here because of issue 1386: that issue is three sites documenting a
///   stale-node verdict they could not produce, and the same sweep found this
///   one producing it without saying so.
///
/// # Safety
/// * `subscription` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_subscription_fini(subscription: *mut rcl_subscription_t) -> rcl_ret_t {
    validate_not_null!(subscription);

    let subscription = &mut *subscription;

    if subscription.state != rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }

    subscription.handle_id = usize::MAX;
    // issue 1437 — the index and the arena it indexes are ONE fact;
    // clearing half would leave a stale executor pointer paired with
    // "not registered".
    subscription._executor = ptr::null_mut();
    subscription.node = crate::node::rclc_node_ref_t::none();
    subscription.state = rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_SHUTDOWN;

    RCL_RET_OK
}

impl rcl_subscription_t {
    /// Can this handle be USED? — the one predicate behind both
    /// `rcl_subscription_is_valid` and `rcl_subscription_get_topic_name`.
    ///
    /// phase-417 stage 3, ledger row `c:subscription_is_valid`. Until
    /// phase-487 a polled subscription was a third live state of this struct,
    /// and a predicate testing `INITIALIZED` alone rejected it; the polled form
    /// is its own type now, so `INITIALIZED` is the whole of "usable" here.
    pub(crate) const fn is_usable(&self) -> bool {
        matches!(
            self.state,
            rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED
        )
    }
}

/// Get the topic name of a subscription.
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
///
/// # Returns
/// * Pointer to topic name (null-terminated), or NULL if the handle is not
///   usable (see `rcl_subscription_t::is_usable`) or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_subscription_get_topic_name(
    subscription: *const rcl_subscription_t,
) -> *const c_char {
    if subscription.is_null() {
        return ptr::null();
    }

    let subscription = &*subscription;
    if !subscription.is_usable() {
        return ptr::null();
    }

    subscription.topic_name.as_ptr() as *const c_char
}

/// How many publishers are on this subscription's topic, right now.
///
/// rcl's `rcl_subscription_get_publisher_count` — phase-444, ledger row
/// `c:subscription_get_publisher_count`. The subscription half of
/// [`rcl_publisher_get_subscription_count`](crate::publisher::rcl_publisher_get_subscription_count),
/// which states the weakening (topic-wide, not matched-to-this-entity), why
/// the executor is a parameter, and why `RCL_RET_UNSUPPORTED` is not `0`.
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
/// * `executor` - The executor whose session sees the graph
/// * `out_count` - Receives the count
///
/// # Safety
/// * `subscription` and `executor` must be valid pointers or NULL
/// * `out_count` must point to a writable `size_t` or be NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_subscription_get_publisher_count(
    subscription: *const rcl_subscription_t,
    executor: *mut crate::executor::rclc_executor_t,
    out_count: *mut usize,
) -> rcl_ret_t {
    if subscription.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription = &*subscription;
    if !subscription.is_usable() {
        return RCL_RET_NOT_INIT;
    }
    crate::executor::rclc_executor_count_publishers(
        executor,
        subscription.topic_name.as_ptr() as *const c_char,
        out_count,
    )
}

/// Is this subscription handle usable?
///
/// rcl's `rcl_subscription_is_valid`, whose contract is "true for any handle
/// that can be used" — the ported idiom is a guard. See
/// `rcl_subscription_t::is_usable` for which states those are.
///
/// # Parameters
/// * `subscription` - Pointer to a subscription
///
/// # Returns
/// * `true` if usable, `false` if finalised, uninitialised or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_subscription_is_valid(
    subscription: *const rcl_subscription_t,
) -> bool {
    if subscription.is_null() {
        return false;
    }

    (*subscription).is_usable()
}

// Internal helper methods for executor
impl rcl_subscription_t {
    /// Get the stored QoS as `nros_rmw::QoSProfile`
    pub(crate) fn get_qos_settings(&self) -> nros_rmw::QoSProfile {
        self.qos.to_qos_settings()
    }

    /// Record the arena entry this subscription was registered into.
    ///
    /// issue 1437 — ONE setter, taking BOTH halves. It used to be
    /// `set_handle_id(id)` alone, and the executor pointer that makes the
    /// index resolvable was simply not stored; three call sites would each
    /// have had to remember the second assignment, which is how a mirror
    /// drifts. A single setter cannot be half-called.
    pub(crate) fn set_arena_entry(&mut self, id: nros_node::HandleId, executor: *mut c_void) {
        self.handle_id = id.to_raw();
        self._executor = executor;
    }
}

// ============================================================================
// phase-417 stage 3 — the subscription half of the `is_valid` family.
//
// Ledger rows `c:subscription_is_valid` / `c:subscription_get_topic_name`;
// the contract lives on `c:client_is_valid`.
// ============================================================================

#[cfg(test)]
mod validity_tests {
    use super::*;

    /// A polled subscription is its own type since phase-487, so this struct
    /// has one live state.
    const LIVE_STATES: [rclc_subscription_state_t; 1] =
        [rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED];

    const DEAD_STATES: [rclc_subscription_state_t; 2] = [
        rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_UNINITIALIZED,
        rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_SHUTDOWN,
    ];

    fn named_subscription(state: rclc_subscription_state_t) -> rcl_subscription_t {
        let mut s = rcl_subscription_t::default();
        s.topic_name[..6].copy_from_slice(b"/chatt");
        s.topic_name_len = 6;
        s.state = state;
        s
    }

    #[test]
    fn subscription_is_valid_accepts_every_state_a_subscription_works_in() {
        for state in LIVE_STATES {
            let s = named_subscription(state);
            assert!(
                unsafe { rcl_subscription_is_valid(&s) },
                "{state:?} is a state a subscription receives in"
            );
        }
        for state in DEAD_STATES {
            let s = named_subscription(state);
            assert!(!unsafe { rcl_subscription_is_valid(&s) }, "{state:?}");
        }
        assert!(!unsafe { rcl_subscription_is_valid(ptr::null()) });
    }

    #[test]
    fn subscription_name_is_readable_in_every_state_it_works_in() {
        for state in LIVE_STATES {
            let s = named_subscription(state);
            assert!(
                !unsafe { rcl_subscription_get_topic_name(&s) }.is_null(),
                "{state:?}"
            );
        }
        for state in DEAD_STATES {
            let s = named_subscription(state);
            assert!(
                unsafe { rcl_subscription_get_topic_name(&s) }.is_null(),
                "{state:?}"
            );
        }
        assert!(unsafe { rcl_subscription_get_topic_name(ptr::null()) }.is_null());
    }

    #[test]
    fn the_subscription_predicate_and_its_accessor_agree_in_every_state() {
        for state in LIVE_STATES.iter().chain(DEAD_STATES.iter()) {
            let s = named_subscription(*state);
            assert_eq!(
                unsafe { rcl_subscription_is_valid(&s) },
                !unsafe { rcl_subscription_get_topic_name(&s) }.is_null(),
                "{state:?}: is_valid and get_topic_name must answer alike"
            );
        }
    }

    /// The publisher is the CONTROL for this class: its state enum has no
    /// third live value, so `INITIALIZED` really is the whole of "usable" and
    /// its predicate is correct as written. Pinned so a future state added to
    /// `rclc_publisher_state_t` cannot quietly re-create the defect.
    #[test]
    fn the_publisher_has_no_third_live_state_to_miss() {
        use crate::publisher::{rcl_publisher_is_valid, rcl_publisher_t, rclc_publisher_state_t};
        let mut p = rcl_publisher_t::default();
        for state in [
            rclc_publisher_state_t::RCLC_PUBLISHER_STATE_UNINITIALIZED,
            rclc_publisher_state_t::RCLC_PUBLISHER_STATE_SHUTDOWN,
        ] {
            p.state = state;
            assert!(!unsafe { rcl_publisher_is_valid(&p) }, "{state:?}");
        }
        p.state = rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED;
        assert!(unsafe { rcl_publisher_is_valid(&p) });
    }
}
