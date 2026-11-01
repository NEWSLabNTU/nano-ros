//! Subscription FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): caller's opaque storage holds a
//! bare `RmwSubscriber` handle. Topic name lives on the C++
//! `rclcpp::Subscription<M>` class. Received CDR bytes are copied directly
//! into the caller's output buffer — no runtime 1 KiB scratch buffer.

use core::ffi::{c_char, c_void};

use nros_rmw::{Session, Subscription as SubscriberTrait, TopicInfo, TransportError};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};
use nros_node::executor::ffi::ExecutorFfi as _;

/// Issue 1425 -- how many takes THIS path has thrown away for being too big.
///
/// A second counter beside the record's `samples_dropped_too_small`, and not a
/// duplicate of it: this one decides the log THROTTLE, and the record's is
/// compiled out unless the image asked for the report. Reading the throttle off
/// the record would make the diagnostic depend on the diagnostic -- an image
/// without `NROS_BOOT_REPORT` would see every drop logged, which is the log
/// flood the throttle exists to prevent (issue 0371's shape).
///
/// A COUNTER rather than a per-entity field, for the reason
/// `nros_node::executor::arena`'s `DROPPED_TAKES` is one: the C++ storage is a
/// bare `RmwSubscriber` and the topic lives on the C++ `Subscription<M>` class,
/// so there is no per-entity place here to put one without changing the ABI.
///
/// `portable_atomic`, never `core::sync::atomic` (issue 2206): the throttle
/// needs `fetch_add`, which a CAS-less target (thumbv6m, a Cortex-M0+ Zephyr
/// image) has only through the polyfill this crate's manifest selects there.
static SAMPLES_DROPPED_TOO_SMALL: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

/// Say that a C++ take was thrown away, and put it in the boot report.
///
/// ISSUE 1425, AND THE HALF OF 0757 THAT NEVER LANDED. `arena.rs` counts and
/// logs the drop for the RUST arena dispatch; this path -- the one every C++
/// `Subscription<M>::take` goes through -- counted nothing and said nothing.
/// It returned `NROS_CPP_RET_FULL` with `out_len = 0`, which the generated C++
/// wrapper reads as "no message waiting", so an undersized buffer is
/// indistinguishable from an idle topic. The transport has already completed
/// and ACKed the sample by then, so every outside probe reports the
/// subscription matched and healthy.
///
/// **What this can and cannot say.** The BUFFER capacity is known here and is
/// the actionable half: it names the knob to raise
/// (`NROS_SUBSCRIPTION_BUFFER_SIZE`). The SAMPLE size is known when the backend
/// knows it -- issue 1612 made a too-small `take` carry the size the sample
/// needed in the span's `len` (`rmw_entity.h`), which reaches this layer as
/// [`SubscriberTrait::refused_sample_len`] -- and printed as `?` when it does
/// not. Zenoh, XRCE, Cyclone and uORB all know it. A `MessageTooLarge` drop is
/// the BACKEND's staging buffer, refused before any take saw a length, so it
/// is always `?`. The topic is still not known: it lives on the C++ class, not
/// in the storage this function is reached from.
///
/// First drop, then every 64th: `arena.rs`'s throttle, on the same reasoning.
/// A forty-participant graph must not turn one misconfigured subscription into
/// a log flood.
///
/// `cpp_diag!` (which is `nros_log`), never stdio: this runs on `no_std`
/// targets and inside Zephyr `native_sim`, where a Rust `std` stdio call is
/// fatal (issue 0589).
///
/// THE LINE IS SHORT ON PURPOSE. `nros_log` formats into a fixed 256-byte
/// stack buffer and its `push_str` is all-or-nothing, so a literal that does
/// not fit in what is LEFT is dropped whole and the record ends in an
/// ellipsis. A paragraph of advice therefore does not become a truncated
/// paragraph, it becomes a line that stops before the knob name -- measured,
/// at 72 characters. Everything a reader needs that is not a number lives in
/// this comment; the line carries the two numbers and the one knob.
#[cold]
fn note_sample_dropped_too_small(err: &TransportError, buf_len: usize, needed: Option<usize>) {
    let n = SAMPLES_DROPPED_TOO_SMALL.fetch_add(1, portable_atomic::Ordering::Relaxed);
    // The RECORD, always -- a board with no console is exactly the case this
    // exists for, and it is the throttled LOG that would tell it nothing.
    nros_node::boot_report::note_sample_dropped_too_small();
    if n != 0 && !n.is_multiple_of(64) {
        return;
    }
    let total = n.saturating_add(1);
    let needed = nros_rmw::RefusedLen(needed);
    crate::cpp_diag!(
        "C++ take DROPPED ({err:?}): {needed}-byte sample, {buf_len}-byte \
         buffer; ACKed then discarded. Raise NROS_SUBSCRIPTION_BUFFER_SIZE to \
         SERIALIZED_SIZE_MAX. {total} dropped so far (issue 1425)."
    );
}

/// The size the backend says the dropped sample needed -- only for the drop it
/// can have measured. Which refusals carry one is `nros_rmw::RefusedLen`'s
/// rule (issue 1632 moved it there, so `nros-node`'s arena drop log reads the
/// same one).
fn refused_len_for(sub: &nros::internals::RmwSubscriber, err: &TransportError) -> Option<usize> {
    nros_rmw::RefusedLen::for_error(err, sub.refused_sample_len()).0
}

/// Create a subscription on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwSubscriber>()` bytes (exposed via `NROS_SUBSCRIBER_SIZE`).
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// appropriately-aligned buffer of at least `NROS_SUBSCRIBER_SIZE` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_create(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || storage.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let topic_str = match unsafe { cstr_to_str(topic) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let node_name_str = core::str::from_utf8(&node_ref.name)
        .ok()
        .and_then(|s| s.split('\0').next());
    let ns_str = core::str::from_utf8(&node_ref.namespace)
        .ok()
        .and_then(|s| s.split('\0').next())
        .unwrap_or("/");

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Phase 305 W3 (issue 0255) — expand `~`/relative topic names against this
    // node's identity and apply its launch remap rules (executor-side table).
    // QoS overrides below keep matching the SOURCE spelling.
    let resolved_topic = match crate::resolve_node_entity_name(ctx, node_ref, topic_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let topic_info = TopicInfo::new(resolved_topic.as_str(), type_str, hash_str)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    let topic_info = match node_name_str {
        Some(name) if !name.is_empty() => topic_info.with_node_name(name),
        _ => topic_info,
    };

    // Phase 211.H (issue #52) — fold any plan qos_overrides for this topic +
    // subscription role, mirroring Rust's `create_subscription_with_qos`.
    let qos_settings = unsafe {
        crate::apply_qos_overrides(
            qos.to_qos_settings(),
            node_ref.qos_overrides,
            node_ref.qos_overrides_len,
            topic_str,
            crate::NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION,
        )
    };

    // Phase 104.C.9.b — when the Node was created via
    // `nros_cpp_node_create_ex` (multi-RMW path), the handle carries a node id
    // and the subscriber must land on the Node's bound session, not
    // the executor's primary. `node_session_mut` resolves both cases
    // transparently.
    let session = match crate::node_id_opt(node_ref) {
        Some(id) => match ctx.executor.node_session_mut(id) {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        },
        None => ctx.executor.session_mut(),
    };

    // phase-454 W10 — the REGISTRATION half of the declared-depth check, for
    // every caller of this seam. C++ components already get a boot-time check
    // in `Node::check_declared_depth` and a compile-time one from
    // `NROS_SUBSCRIBE`; this catches the two that reach neither — a C
    // component, whose configure function calls this symbol directly with a
    // `nros_cpp_qos_t` it built at run time, and a C++ call site that names
    // this FFI rather than the node's own verb.
    if nros_node::declared_qos::check(topic_info.type_name, topic_info.name, &qos_settings).is_err()
    {
        return crate::NROS_CPP_RET_DECLARED_DEPTH_MISMATCH;
    }
    // phase-519 W3b (RFC-0074) — the declared ingress rate, as the executor's
    // registration hands it on.
    let topic_info = nros_node::declared_qos::with_declared_ingress(topic_info);
    match session.create_subscription(&topic_info, qos_settings) {
        Ok(handle) => {
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwSubscriber, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Subscription creation options (phase-402).
///
/// The three register functions differ in exactly one thing — the callback
/// type — and used to duplicate eight parameters to say it. The duplication had
/// already drifted: `callback_group` existed on one of the three and not the
/// other two, for no reason anyone chose. This struct is the axis that grows
/// instead of the argument list.
///
/// `component.h` records what the flat list already cost: a C caller built
/// against the 9-arg shape "left the 11th slot as stack garbage, which the Rust
/// side dereferenced (SIGSEGV in `cstr_to_str` on Zephyr native_sim; silent
/// luck elsewhere)". A NULL options pointer is all-defaults, which is exactly
/// the previous behaviour and cannot be misread as garbage.
///
/// Precedent is issue 0808 on `create_session`, whose resolution `rmw_entity.h`
/// records: take one options struct, because
/// `rmw_{publisher,subscription}_options_t` already solved this for entities.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_subscription_options_t {
    /// Receive-buffer size hint in bytes; `0` = use the image default.
    ///
    /// Issue 0896: a subscription that states none takes the small size class
    /// whatever its message type. Codegen's `{Msg}_RX_MAX_SERIALIZED_SIZE` is
    /// the number to pass.
    pub rx_buffer_hint: u32,
    /// Scheduling-context slot; `0` = inherit the executor default.
    pub sched_context: u8,
    /// Reserved; must be zero.
    pub _reserved: [u8; 3],
    /// Callback group name; NULL = default. Available on ALL THREE register
    /// variants now, which it was not before.
    pub callback_group: *const c_char,
}

// Not derived: that needs `Default` for a raw pointer, which is Rust 1.88, and
// this crate's clippy MSRV is 1.87 (`clippy.toml`). The pinned stable clippy
// honours that; the nightly one the NuttX build-std lint uses does not.
#[allow(clippy::derivable_impls)]
impl Default for nros_cpp_subscription_options_t {
    fn default() -> Self {
        Self {
            rx_buffer_hint: 0,
            sched_context: 0,
            _reserved: [0; 3],
            callback_group: core::ptr::null(),
        }
    }
}

/// A zero-initialised [`nros_cpp_subscription_options_t`] — every field
/// "inherit"/"off", identical to passing NULL.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_subscription_default_options() -> nros_cpp_subscription_options_t {
    nros_cpp_subscription_options_t::default()
}

/// Read a NULLable options pointer into `(sched_context, callback_group,
/// rx_buffer_hint)`.
///
/// ONE reader, shared by all three register variants, so a new field cannot be
/// honoured by one and ignored by the others — which is the drift that left
/// `callback_group` on one of three.
///
/// # Safety
/// `options` is NULL or points at a valid `nros_cpp_subscription_options_t`.
unsafe fn read_subscription_options(
    options: *const nros_cpp_subscription_options_t,
) -> (u8, *const c_char, u32) {
    if options.is_null() {
        return (0, core::ptr::null(), 0);
    }
    let o = unsafe { &*options };
    (o.sched_context, o.callback_group, o.rx_buffer_hint)
}

/// Phase 189.M3.x — register a **callback-style** subscription in the executor
/// arena (rclcpp dispatch model), as opposed to the poll-style
/// `nros_cpp_subscription_create` above. The arena owns the subscriber; spin
/// dispatches `callback(cdr_bytes)` on each new sample. Returns the executor
/// `HandleId` via `out_handle_id` (for cancel / introspection) and, when
/// `sched_context != 0`, binds that handle to the scheduling context (poll-style
/// subscriptions have no dispatched callback to schedule). `callback` is the C++
/// template's raw trampoline; `context` is the `rclcpp::Subscription<M>` object
/// (`this`). Mirrors `nros_cpp_service_server_register` one entity over.
///
/// # Safety
/// All non-NULL pointers must be valid; `callback` must be a valid trampoline;
/// `context` outlives the executor (no move after register).
///
/// Phase 273 (RFC-0047): `callback_group` is an optional null-terminated group
/// name (NULL or empty string ⇒ default group ⇒ node default SchedContext).
/// Phase 402 moved it, `sched_context` and `rx_buffer_hint` onto
/// [`nros_cpp_subscription_options_t`]; a NULL `options` is all-defaults and is
/// exactly what callers used to get by passing `0` and NULL here.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
unsafe fn subscription_register_impl(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    callback: nros_node::RawSubscriptionCallback,
    context: *mut c_void,
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
    capture: Option<&[u8]>,
    capture_drop: Option<unsafe extern "C" fn(*mut c_void)>,
) -> nros_cpp_ret_t {
    // phase-402 — ONE reader for all three variants; see
    // `read_subscription_options`.
    let (sched_context, callback_group, rx_buffer_hint) =
        unsafe { read_subscription_options(options) };
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || out_handle_id.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let topic_str = match unsafe { cstr_to_str(topic) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    // Phase 273 — extract group name from the optional C string.
    let group_str = if callback_group.is_null() {
        None
    } else {
        let s = unsafe { cstr_to_str(callback_group) }.unwrap_or("");
        if s.is_empty() { None } else { Some(s) }
    };

    // The arena registration derives namespace + node name from the node record
    // (`add_arena_subscription_c_callback`), so only the bare topic is passed
    // here — unlike the poll-style create which builds TopicInfo itself.
    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Phase 305 W3 (issue 0255) — resolve `~`/relative names + launch remaps
    // against this node's identity before registering.
    let resolved_topic = match crate::resolve_node_entity_name(ctx, node_ref, topic_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let topic_str = resolved_topic.as_str();

    use nros_node::config::DEFAULT_RX_BUF_SIZE as BUF;
    // Issue 0312 — a single-node entry's only node is `NodeId(0)`; reading the
    // field raw made it indistinguishable from "unset", so the arena
    // registration below fell back to the executor's own (empty) node name and
    // the subscription never got a liveliness token.
    let node_id = crate::node_id_opt(node_ref);
    let result = ctx
        .executor
        .add_arena_subscription_c_callback_with_capture::<BUF>(
            node_id,
            topic_str,
            type_str,
            hash_str,
            qos.to_qos_settings(),
            callback,
            context,
            group_str,
            // phase-402 W2 / issue 0896 — THE point of the options struct: the
            // caller's per-type receive bound finally reaches the backend, which
            // routes the payload size class on it. 0 = no opinion, same as before.
            rx_buffer_hint as usize,
            capture,
            capture_drop,
        );

    match result {
        Ok(handle_id) => {
            // Bind the (now-real) arena handle to the requested sched context.
            // `0` = inherit (no-op); unknown slot fails.
            if sched_context != 0 {
                let sc_id = nros_node::executor::sched_context::SchedContextId(sched_context);
                if ctx
                    .executor
                    .bind_handle_to_sched_context(handle_id, sc_id)
                    .is_err()
                {
                    return NROS_CPP_RET_INVALID_ARGUMENT;
                }
            }
            crate::issue_owned_handle(ctx.executor, node_ref, handle_id, out_handle_id);
            NROS_CPP_RET_OK
        }
        Err(e) => crate::node_error_to_cpp_ret(e),
    }
}

/// Phase 189.M3.x — the callback-style registration, unchanged.
///
/// `context` is passed through to the dispatch as-is, so the caller owns
/// whatever it points at and the old contract applies: no move after register.
/// `nros_cpp_subscription_register_capturing` is the variant that lifts that.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_subscription_register(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    callback: nros_node::RawSubscriptionCallback,
    context: *mut c_void,
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
) -> nros_cpp_ret_t {
    unsafe {
        subscription_register_impl(
            node,
            topic,
            type_name,
            type_hash,
            qos,
            callback,
            context,
            out_handle_id,
            options,
            None,
            None,
        )
    }
}

/// phase-456 W1 — register a callback-style subscription whose CAPTURE the
/// arena holds.
///
/// The difference from `nros_cpp_subscription_register` is the lifetime, not
/// the dispatch. There, `context` is the caller's and the caller must keep it
/// alive and unmoved for as long as the executor runs — which is why a C++
/// `Subscription<M>` had to be immovable after registration, and why the
/// returning `create_subscription` had to heap-allocate a cell to hold a
/// capturing lambda at a stable address.
///
/// Here the first `capture_len` bytes at `capture` are COPIED into the arena
/// entry, and the dispatch context becomes the entry's own copy. Nothing of the
/// caller's is referenced after this returns, so the caller may keep nothing at
/// all — which is the point: the C++ side becomes a handle.
///
/// **`capture_len` is a LENGTH, not a budget — phase-456 W8.** W1 copied the
/// capture into a fixed `[u8; CALLBACK_CAPTURE_BYTES]` inside the arena entry
/// and refused anything longer, where that constant had to equal the C++
/// `NROS_CPP_CALLBACK_CAPACITY` macro by construction; the refusal was
/// therefore reachable only when the two languages had drifted apart about a
/// number. The runtime now bump-allocates exactly `capture_len` bytes from the
/// arena, so the only bound left is the arena every other entry already shares,
/// and a registration that exhausts it fails the way every other arena
/// allocation does.
///
/// `capture_len == 0` (or a NULL `capture`) is exactly
/// `nros_cpp_subscription_register`.
///
/// **phase-476 W2 — `capture_drop`.** Copying the capture's bytes MOVES whatever
/// the capture owns, so the caller relinquishes its own copy without destroying
/// it, and `capture_drop` (when non-NULL) is called on the arena's copy when the
/// subscription is released. Before W2 the caller destroyed its copy right
/// after this returned while the arena kept dispatching through the bytes, so a
/// capture that owned anything (a `std::shared_ptr`, a `std::string`) was
/// destroyed while still in use.
///
/// # Safety
/// All non-NULL pointers must be valid; `callback` must be a valid trampoline
/// whose context is the captured bytes; `capture` must point to `capture_len`
/// readable bytes for the duration of THIS CALL only.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_subscription_register_capturing(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    callback: nros_node::RawSubscriptionCallback,
    capture: *const u8,
    capture_len: usize,
    capture_drop: Option<unsafe extern "C" fn(*mut c_void)>,
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
) -> nros_cpp_ret_t {
    let bytes = if capture.is_null() || capture_len == 0 {
        None
    } else {
        Some(unsafe { core::slice::from_raw_parts(capture, capture_len) })
    };
    unsafe {
        subscription_register_impl(
            node,
            topic,
            type_name,
            type_hash,
            qos,
            callback,
            core::ptr::null_mut(),
            out_handle_id,
            options,
            bytes,
            capture_drop,
        )
    }
}

/// Phase 189.M3.4 — register a **callback-style** subscription that also delivers
/// the sample's wire **attachment**. The callback analogue of the poll-side
/// `nros_cpp_subscription_take_serialized_with_attachment` (M3.4b), and the C++
/// mirror of the C `rclc_executor_add_subscription_raw_with_info`: same
/// arena dispatch + sched binding as `nros_cpp_subscription_register`, but routed
/// through `add_arena_subscription_c_info_callback` so the trampoline receives
/// `(data, len, attachment, attachment_len, ctx)`. Cross-RMW bridges read the
/// bridge_origin tag from the attachment.
///
/// # Safety
/// All non-NULL pointers must be valid; `callback` must be a valid trampoline;
/// `context` outlives the executor (no move after register).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_subscription_register_with_info(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    callback: nros_node::executor::RawSubscriptionInfoCallback,
    context: *mut c_void,
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
) -> nros_cpp_ret_t {
    // phase-402 — these two could not take a callback group at all before.
    let (sched_context, callback_group, rx_buffer_hint) =
        unsafe { read_subscription_options(options) };
    let _ = callback_group;
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || out_handle_id.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let topic_str = match unsafe { cstr_to_str(topic) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Phase 305 W3 (issue 0255) — resolve `~`/relative names + launch remaps.
    let resolved_topic = match crate::resolve_node_entity_name(ctx, node_ref, topic_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let topic_str = resolved_topic.as_str();

    use nros_node::config::DEFAULT_RX_BUF_SIZE as BUF;
    let node_id = if crate::node_id_opt(node_ref).is_some() {
        Some(crate::node_id_opt(node_ref).expect("guarded above"))
    } else {
        None
    };
    let result = ctx.executor.add_arena_subscription_c_info_callback::<BUF>(
        node_id,
        topic_str,
        type_str,
        hash_str,
        qos.to_qos_settings(),
        callback,
        context,
        // phase-408 W5a — the hint reaches the backend's payload
        // classing instead of being destructured and dropped.
        rx_buffer_hint as usize,
    );

    match result {
        Ok(handle_id) => {
            if sched_context != 0 {
                let sc_id = nros_node::executor::sched_context::SchedContextId(sched_context);
                if ctx
                    .executor
                    .bind_handle_to_sched_context(handle_id, sc_id)
                    .is_err()
                {
                    return NROS_CPP_RET_INVALID_ARGUMENT;
                }
            }
            crate::issue_owned_handle(ctx.executor, node_ref, handle_id, out_handle_id);
            NROS_CPP_RET_OK
        }
        Err(e) => crate::node_error_to_cpp_ret(e),
    }
}

/// Issue 2036 — one sample's metadata, upstream's `rmw_message_info_t` field
/// for field (`rclcpp::MessageInfo::get_rmw_message_info()` is filled from
/// it). A backend that reports none (XRCE, uORB) hands every field zeroed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nros_cpp_message_info_t {
    /// Publisher's clock at publication, ns; 0 = not reported.
    pub source_timestamp: i64,
    /// Subscriber's clock at reception, ns; 0 = not reported.
    pub received_timestamp: i64,
    /// The publisher's sequence number for this sample; 0 = not reported.
    pub publication_sequence_number: u64,
    /// This subscription's reception count; 0 = not reported.
    pub reception_sequence_number: u64,
    /// The publisher's GID, zero-padded to `RMW_GID_STORAGE_SIZE` (24); all
    /// zero = not reported. How many bytes mean anything is backend-specific
    /// (`nros_core::MessageInfo::publisher_gid`).
    pub publisher_gid: [u8; 24],
    /// `true` when the backend reported metadata for this sample at all.
    pub has_info: bool,
}

impl nros_cpp_message_info_t {
    fn from_info(info: Option<&nros::MessageInfo>) -> Self {
        match info {
            Some(i) => Self {
                source_timestamp: i.source_timestamp().to_nanos(),
                received_timestamp: i.received_timestamp().to_nanos(),
                publication_sequence_number: i.publication_sequence_number() as u64,
                reception_sequence_number: i.reception_sequence_number() as u64,
                publisher_gid: *i.publisher_gid(),
                has_info: true,
            },
            None => Self {
                source_timestamp: 0,
                received_timestamp: 0,
                publication_sequence_number: 0,
                reception_sequence_number: 0,
                publisher_gid: [0; 24],
                has_info: false,
            },
        }
    }
}

/// Issue 2036 — the C++ side of a MessageInfo subscription: the invoker, the
/// heap capture it is handed, and how to free that capture. Owned by the
/// arena entry's closure, so the capture is freed exactly once, when the
/// subscription is released.
struct CppMessageInfoSink {
    invoke: unsafe extern "C" fn(*const u8, usize, *const nros_cpp_message_info_t, *mut c_void),
    context: *mut c_void,
    drop: unsafe extern "C" fn(*mut c_void),
}

impl Drop for CppMessageInfoSink {
    fn drop(&mut self) {
        // SAFETY: `context` is the capture the C++ side allocated for this
        // registration, and this is its one owner.
        unsafe { (self.drop)(self.context) };
    }
}

/// Register a **callback-style** subscription whose callback receives each
/// sample's metadata — issue 2036, the runtime half of the C++
/// `(const M&, const rclcpp::MessageInfo&)` subscription callback (hosted).
///
/// `invoke(data, len, info, context)` runs during `spin_once` with the CDR
/// bytes and an [`nros_cpp_message_info_t`] filled from the backend's
/// `take_serialized_with_info`. `context` is a heap capture the registration
/// OWNS from here on: `context_drop(context)` runs once, when the
/// subscription is released (or right away if registration fails).
///
/// # Safety
/// All pointers must be valid; `invoke` and `context_drop` must be callable
/// for the life of the subscription.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_subscription_register_message_info(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    invoke: unsafe extern "C" fn(*const u8, usize, *const nros_cpp_message_info_t, *mut c_void),
    context: *mut c_void,
    context_drop: unsafe extern "C" fn(*mut c_void),
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
) -> nros_cpp_ret_t {
    // Owns the capture from the first line: every early return frees it.
    let sink = CppMessageInfoSink {
        invoke,
        context,
        drop: context_drop,
    };
    let (sched_context, callback_group, rx_buffer_hint) =
        unsafe { read_subscription_options(options) };
    let _ = callback_group;
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || out_handle_id.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(topic_str) = (unsafe { cstr_to_str(topic) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(type_str) = (unsafe { cstr_to_str(type_name) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment.
    let Some(hash_str) = (unsafe { cstr_to_str(type_hash) }).map(crate::normalize_type_hash) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };
    let resolved_topic = match crate::resolve_node_entity_name(ctx, node_ref, topic_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let node_id = crate::node_id_opt(node_ref);
    let result = ctx.executor.add_arena_subscription_msg_info_closure(
        node_id,
        resolved_topic.as_str(),
        type_str,
        hash_str,
        qos.to_qos_settings(),
        rx_buffer_hint as usize,
        move |data: &[u8], info: Option<&nros::MessageInfo>| {
            // Name the WHOLE `sink`: edition-2021 closures capture disjoint
            // fields, and capturing only the two `Copy` fields would leave
            // `sink` (and its `Drop`, which frees the capture) behind in this
            // frame — freed at return, called after.
            let sink = &sink;
            let c_info = nros_cpp_message_info_t::from_info(info);
            // SAFETY: `invoke` / `context` are the C++ side's, alive while the
            // entry (and so this closure, which owns `sink`) is.
            unsafe { (sink.invoke)(data.as_ptr(), data.len(), &c_info, sink.context) };
        },
    );
    match result {
        Ok(handle_id) => {
            if sched_context != 0 {
                let sc_id = nros_node::executor::sched_context::SchedContextId(sched_context);
                if ctx
                    .executor
                    .bind_handle_to_sched_context(handle_id, sc_id)
                    .is_err()
                {
                    return NROS_CPP_RET_INVALID_ARGUMENT;
                }
            }
            crate::issue_owned_handle(ctx.executor, node_ref, handle_id, out_handle_id);
            NROS_CPP_RET_OK
        }
        Err(e) => crate::node_error_to_cpp_ret(e),
    }
}

/// Phase 269 W3 — register a **callback-style** subscription that surfaces the
/// sample's E2E integrity status (CRC + sequence gap/dup) alongside the CDR bytes
/// — the C/C++ component-callback analog of Rust's
/// `register_subscription_buffered_raw_safety_on` / `CallbackCtx::integrity()`.
///
/// The executor validates each sample via `take_validated` and unpacks the
/// [`nros_rmw::IntegrityStatus`] into three plain scalars (gap, duplicate,
/// crc_valid) before invoking the callback:
/// ```c
/// void callback(data, len, gap, duplicate, crc_valid, ctx)
/// ```
/// Arena-registered like `nros_cpp_subscription_register`; `sched_context != 0`
/// binds the handle to the requested scheduling tier. The C++ `subscription.hpp`
/// declares a matching `nros_cpp_subscription_validated_callback_t` typedef and a
/// `message_safety_trampoline` that packs the scalars back into
/// `nros_cpp_integrity_status_t` for the typed user handler.
///
/// Requires `safety-e2e` on both the Rust runtime AND the CMake build
/// (`NROS_SAFETY_E2E=ON`, lowered from `[system].features = ["safety"]` via
/// `NanoRosCapabilities.cmake:64`).
///
/// # Safety
/// All non-NULL pointers must be valid; `callback` must be a valid trampoline;
/// `context` outlives the executor (no move after register).
#[cfg(feature = "safety-e2e")]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_subscription_register_validated(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    callback: nros_node::executor::RawSubscriptionSafetyCallback,
    context: *mut c_void,
    out_handle_id: *mut usize,
    options: *const nros_cpp_subscription_options_t,
) -> nros_cpp_ret_t {
    // phase-402 — these two could not take a callback group at all before.
    let (sched_context, callback_group, rx_buffer_hint) =
        unsafe { read_subscription_options(options) };
    let _ = callback_group;
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || out_handle_id.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let topic_str = match unsafe { cstr_to_str(topic) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Phase 305 W3 (issue 0255) — resolve `~`/relative names + launch remaps.
    let resolved_topic = match crate::resolve_node_entity_name(ctx, node_ref, topic_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let topic_str = resolved_topic.as_str();

    use nros_node::config::DEFAULT_RX_BUF_SIZE as BUF;
    let node_id = if crate::node_id_opt(node_ref).is_some() {
        Some(crate::node_id_opt(node_ref).expect("guarded above"))
    } else {
        None
    };
    let result = ctx
        .executor
        .add_arena_subscription_c_validated_callback::<BUF>(
            node_id,
            topic_str,
            type_str,
            hash_str,
            qos.to_qos_settings(),
            callback,
            context,
            // phase-408 W5a — the hint reaches the backend's payload
            // classing instead of being destructured and dropped.
            rx_buffer_hint as usize,
        );

    match result {
        Ok(handle_id) => {
            if sched_context != 0 {
                let sc_id = nros_node::executor::sched_context::SchedContextId(sched_context);
                if ctx
                    .executor
                    .bind_handle_to_sched_context(handle_id, sc_id)
                    .is_err()
                {
                    return NROS_CPP_RET_INVALID_ARGUMENT;
                }
            }
            crate::issue_owned_handle(ctx.executor, node_ref, handle_id, out_handle_id);
            NROS_CPP_RET_OK
        }
        Err(e) => crate::node_error_to_cpp_ret(e),
    }
}

/// Try to receive raw CDR data from a subscription (non-blocking).
///
/// Writes the received CDR bytes directly into the caller's output buffer
/// — no runtime scratch. If the message is larger than `out_capacity`
/// the backend drops it and returns `NROS_CPP_RET_FULL`; callers that need
/// to handle oversized messages should size `out_data` to the message type's
/// `SERIALIZED_SIZE_MAX` (exactly what `Subscription<M>::take` does).
///
/// # Safety
/// `storage` must be a valid subscription storage. `out_data` must point to
/// `out_capacity` writable bytes. `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_take_serialized(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };

    match sub.take_serialized(out_slice) {
        Ok(Some(len)) => {
            unsafe {
                *out_len = len;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Err(e @ (TransportError::BufferTooSmall | TransportError::MessageTooLarge)) => {
            // The backend drops the oversized message; `out_len` stays 0
            // because the backend doesn't report the actual length. Issue 1425
            // -- COUNTED before the return, because `out_len = 0` with
            // `NROS_CPP_RET_FULL` is what the C++ wrapper reads as "nothing
            // waiting".
            note_sample_dropped_too_small(&e, out_capacity, refused_len_for(sub, &e));
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_FULL
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 252 / issue 0073 — E2E message-integrity status for the C++ receive
/// path ([`nros_cpp_subscription_take_validated`]). The C++ analog of the
/// Rust `IntegrityStatus` / `rclc_integrity_status_t`.
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub struct nros_cpp_integrity_status_t {
    /// Sequence-number gap since the previous in-order message (0 = none).
    pub gap: i64,
    /// `true` if this sample's sequence number was already seen (a duplicate).
    pub duplicate: bool,
    /// CRC verdict: `1` = valid, `0` = mismatch, `-1` = no CRC on the wire.
    pub crc_valid: i8,
}

/// Phase 252 / issue 0073 — non-blocking poll that ALSO returns the E2E integrity
/// status (CRC + sequence gap/dup). The safety-e2e analog of
/// [`nros_cpp_subscription_take_serialized`]; the backend recomputes + compares the
/// CRC attachment and tracks the sequence. Requires `safety-e2e` on both ends
/// (else `crc_valid` reports `-1`).
///
/// # Safety
/// `storage` must be a valid subscription storage. `out_data` must point to
/// `out_capacity` writable bytes. `out_len` must be valid. `out_status`, if
/// non-NULL, must point to a writable `nros_cpp_integrity_status_t`.
#[cfg(feature = "safety-e2e")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_take_validated(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
    out_status: *mut nros_cpp_integrity_status_t,
) -> nros_cpp_ret_t {
    if storage.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };

    match sub.take_validated(out_slice) {
        Ok(Some((len, status))) => {
            unsafe {
                *out_len = len;
                if !out_status.is_null() {
                    *out_status = nros_cpp_integrity_status_t {
                        gap: status.gap,
                        duplicate: status.duplicate,
                        crc_valid: match status.crc_valid {
                            Some(true) => 1,
                            Some(false) => 0,
                            None => -1,
                        },
                    };
                }
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Err(e @ (TransportError::BufferTooSmall | TransportError::MessageTooLarge)) => {
            // Issue 1425, as on the plain take above: the safety-e2e flavour of
            // this path drops on exactly the same condition, so leaving it
            // uncounted would make the tally depend on which feature the image
            // was built with.
            note_sample_dropped_too_small(&e, out_capacity, refused_len_for(sub, &e));
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_FULL
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 189.M3.4b — try to receive raw CDR data **plus the sample's wire
/// attachment** (non-blocking). The C++ poll-side analog of the Rust
/// `node.subscription(t).generic(..).message_info()` builder: writes the
/// payload into `out_data` and the attachment into `out_att`. `*out_att_len`
/// is `0` when the sample carried no attachment. Cross-RMW bridges read the
/// bridge_origin tag from the attachment.
///
/// # Safety
/// `storage` must be a valid subscription storage. `out_data` /`out_att` must
/// point to `out_capacity` / `out_att_capacity` writable bytes. `out_len` /
/// `out_att_len` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_take_serialized_with_attachment(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
    out_att: *mut u8,
    out_att_capacity: usize,
    out_att_len: *mut usize,
) -> nros_cpp_ret_t {
    if storage.is_null()
        || out_data.is_null()
        || out_len.is_null()
        || out_att.is_null()
        || out_att_len.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };
    let att_slice = unsafe { core::slice::from_raw_parts_mut(out_att, out_att_capacity) };

    match sub.take_serialized_with_attachment(out_slice, att_slice) {
        Ok(Some((len, att_len))) => {
            unsafe {
                *out_len = len;
                *out_att_len = att_len;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
                *out_att_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Err(e @ (TransportError::BufferTooSmall | TransportError::MessageTooLarge)) => {
            // Issue 1425 -- the third of the three C++ take flavours, counted
            // for the same reason as the two above. `out_capacity` is the
            // PAYLOAD buffer: the attachment has its own, but a sample that
            // overran the attachment buffer is a different fault with a
            // different knob, and naming one size for both would send the
            // reader to the wrong one.
            note_sample_dropped_too_small(&e, out_capacity, refused_len_for(sub, &e));
            unsafe {
                *out_len = 0;
                *out_att_len = 0;
            }
            NROS_CPP_RET_FULL
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Issue 2074 — the POLL-side take with metadata: upstream's
/// `rclcpp::Subscription::take(msg, message_info)`. Writes the CDR bytes into
/// `out_data` and the sample's metadata into `*out_info`, both from ONE
/// `take_serialized_with_info`, the take the callback-side MessageInfo path
/// (issue 2036) uses. `*out_len` is `0` and `*out_info` is zeroed (with
/// `has_info = false`) when nothing was waiting.
///
/// `out_info->has_info` tells "the backend reported metadata for this sample"
/// apart from "it reported none" (XRCE, uORB), which an all-zero record
/// cannot.
///
/// # Safety
/// `storage` must be a valid subscription storage. `out_data` must point to
/// `out_capacity` writable bytes. `out_len` and `out_info` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_take_with_info(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
    out_info: *mut nros_cpp_message_info_t,
) -> nros_cpp_ret_t {
    if storage.is_null() || out_data.is_null() || out_len.is_null() || out_info.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };

    match sub.take_serialized_with_info(out_slice) {
        Ok(Some((len, info))) => {
            unsafe {
                *out_len = len;
                *out_info = nros_cpp_message_info_t::from_info(info.as_ref());
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
                *out_info = nros_cpp_message_info_t::from_info(None);
            }
            NROS_CPP_RET_OK
        }
        Err(e @ (TransportError::BufferTooSmall | TransportError::MessageTooLarge)) => {
            // Issue 1425 -- counted like the other take flavours.
            note_sample_dropped_too_small(&e, out_capacity, refused_len_for(sub, &e));
            unsafe {
                *out_len = 0;
                *out_info = nros_cpp_message_info_t::from_info(None);
            }
            NROS_CPP_RET_FULL
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

// ============================================================================
// Phase 124.A.7 — zero-copy subscription borrow / release
// ============================================================================

/// Phase 124.A.7 — borrow the next message in place.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `nros_cpp_publisher_loan`. Issue 0814.
///
/// On success, `*out_buf` points at `*out_len` bytes (read-only) until
/// the caller calls `nros_cpp_subscription_release(storage, token)`.
///
/// Returns `> 0` (length), `0` (no message), or negative error code.
///
/// # Safety
/// All pointer parameters must be valid. Only one outstanding borrow
/// per subscription is allowed.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_borrow(
    storage: *mut c_void,
    out_buf: *mut *const u8,
    out_len: *mut usize,
    out_token: *mut *mut c_void,
) -> i32 {
    if storage.is_null() || out_buf.is_null() || out_len.is_null() || out_token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_rmw::SlotBorrowing;
    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    match sub.try_borrow() {
        Ok(Some(view)) => {
            let buf_ptr = view.as_ref().as_ptr();
            let len = view.as_ref().len();
            // SAFETY: erase lifetime — caller's release contract ensures
            // the view doesn't outlive the subscription.
            let view_static: nros::internals::RmwView<'static> = unsafe {
                core::mem::transmute::<
                    nros::internals::RmwView<'_>,
                    nros::internals::RmwView<'static>,
                >(view)
            };
            let boxed = alloc::boxed::Box::new(view_static);
            unsafe {
                *out_buf = buf_ptr;
                *out_len = len;
                *out_token = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            len as i32
        }
        Ok(None) => 0,
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Phase 124.A.7 — release a previously borrowed view.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `nros_cpp_publisher_loan`. Issue 0814.
///
/// # Safety
/// `storage` must be the subscription the token was borrowed from.
/// `token` must come from a matching `nros_cpp_subscription_borrow`
/// and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_release(
    storage: *mut c_void,
    token: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() || token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let _view: alloc::boxed::Box<nros::internals::RmwView<'static>> =
        unsafe { alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwView<'static>) };
    NROS_CPP_RET_OK
}

/// Phase 124.D.1 — burst-take.
///
/// Drain up to `max_msgs` queued samples into the contiguous `buf`
/// block in a single call. The i-th delivered sample lives at
/// `buf + i * per_msg_cap` with length `out_lens[i]`. Returns the
/// number of samples delivered (`>= 0`) via `out_count` and an
/// `nros_cpp_ret_t` status:
///   * `NROS_CPP_RET_OK` — `*out_count` was written.
///   * `NROS_CPP_RET_INVALID_ARGUMENT` — null pointer or zero
///     per-message cap.
///   * `NROS_CPP_RET_ERROR` — backend-level transport failure.
///
/// # Safety
/// `storage` must be a valid initialized subscription. `buf` must
/// point to a writable block of `max_msgs * per_msg_cap` bytes.
/// `out_lens` must point to a writable array of `max_msgs` `size_t`
/// slots. `out_count` must be a writable `usize` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_take_sequence(
    storage: *mut c_void,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
    out_count: *mut usize,
) -> nros_cpp_ret_t {
    use nros_rmw::Subscription;
    if storage.is_null() || buf.is_null() || out_lens.is_null() || out_count.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    if per_msg_cap == 0 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let buf_slice =
        unsafe { core::slice::from_raw_parts_mut(buf, max_msgs.saturating_mul(per_msg_cap)) };
    let lens_slice = unsafe { core::slice::from_raw_parts_mut(out_lens, max_msgs) };
    match sub.take_sequence(buf_slice, per_msg_cap, max_msgs, lens_slice) {
        Ok(count) => {
            unsafe {
                *out_count = count;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Release a dispatch subscription — issue 1667: the callback stops, the
/// subscriber leaves the graph, and the arena entry and its capture are freed
/// for the next registration.
///
/// What `SubscriptionHandle<M>::reset()` (upstream `sub_.reset()`) calls. Safe
/// on a stale or copied handle and from inside the subscription's own callback;
/// see `release_dispatch_entry`. Not `_release` like its timer and service
/// twins: `nros_cpp_subscription_release` is the loan-release verb (phase-124).
///
/// # Safety
/// `executor_handle` must be a valid executor handle (one `nros_cpp_fini`
/// already finalised is refused).
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_unregister(
    executor_handle: *mut c_void,
    handle_id: usize,
) -> nros_cpp_ret_t {
    unsafe {
        crate::release_dispatch_entry(
            executor_handle,
            handle_id,
            crate::CppExecutor::release_subscription,
        )
    }
}

/// Destroy a subscription (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized subscription storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwSubscriber);
    }
    // Issue 1972 — the backend registration died with the handle.
    nros_c::status_event_slots::release_owner(storage as usize);
    NROS_CPP_RET_OK
}

/// Relocate an `RmwSubscriber` from `old_storage` to `new_storage`.
///
/// Subscriptions are pull-based (`take_serialized`) and register nothing
/// externally that references the storage address — relocation is a
/// straight `ptr::read` + `ptr::write`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwSubscriber);
        core::ptr::write(new_storage as *mut nros::internals::RmwSubscriber, value);
    }
    // Issue 1972 — the one external registration that DOES name the storage:
    // a status-event callback receives it, so its slot follows the move.
    nros_c::status_event_slots::rekey(old_storage as usize, new_storage as usize, new_storage);
    NROS_CPP_RET_OK
}

// ============================================================================
// Poll-subscription status events — Phase 108 (Tier-1 kinds, wired by issue
// 1993) and issue 1972 (matched, incompatible type)
// ============================================================================
//
// User-facing C++ event-setter shims, matching the typedefs in
// `<rclcpp/polling_subscription.hpp>`. Every setter keeps its `(callback,
// context, storage)` triple in nros-c's process-wide `status_event_slots`
// table (16 registrations across every C and C++ entity; one past that is
// `NROS_CPP_RET_FULL`), and the backend decides whether it carries the kind:
// `NROS_CPP_RET_UNSUPPORTED` on XRCE-DDS and uORB.

/// Liveliness payload. Layout-identical to `nros_rmw::LivelinessChangedStatus`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_liveliness_changed_status_t {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

/// Count payload for the subscription-side count kinds (requested deadline
/// missed, message lost). Layout-identical to `nros_rmw::CountStatus`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    use nros_rmw::{CountStatus as R, LivelinessChangedStatus as L};
    // The slot trampoline passes the `nros_rmw` payload BY VALUE.
    assert!(size_of::<nros_cpp_liveliness_changed_status_t>() == size_of::<L>());
    assert!(align_of::<nros_cpp_liveliness_changed_status_t>() == align_of::<L>());
    assert!(
        offset_of!(nros_cpp_liveliness_changed_status_t, alive_count) == offset_of!(L, alive_count)
    );
    assert!(
        offset_of!(nros_cpp_liveliness_changed_status_t, not_alive_count)
            == offset_of!(L, not_alive_count)
    );
    assert!(
        offset_of!(nros_cpp_liveliness_changed_status_t, alive_count_change)
            == offset_of!(L, alive_count_change)
    );
    assert!(
        offset_of!(nros_cpp_liveliness_changed_status_t, not_alive_count_change)
            == offset_of!(L, not_alive_count_change)
    );
    assert!(size_of::<nros_cpp_count_status_t>() == size_of::<R>());
    assert!(align_of::<nros_cpp_count_status_t>() == align_of::<R>());
    assert!(offset_of!(nros_cpp_count_status_t, total_count) == offset_of!(R, total_count));
    assert!(
        offset_of!(nros_cpp_count_status_t, total_count_change)
            == offset_of!(R, total_count_change)
    );
};

/// Callback for `LivelinessChanged`.
pub type nros_cpp_liveliness_changed_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_liveliness_changed_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for the subscription-side count kinds.
pub type nros_cpp_subscriber_count_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_count_status_t,
        user_context: *mut c_void,
    ),
>;

/// Register a liveliness-changed callback on a poll-style subscription —
/// rclcpp's `SubscriptionEventCallbacks::liveliness_callback`, issue 1993.
/// Observed as the subscription is polled (`has_data` / take).
///
/// Returns `NROS_CPP_RET_OK`, `NROS_CPP_RET_INVALID_ARGUMENT` (NULL storage or
/// callback), `NROS_CPP_RET_UNSUPPORTED` when the backend does not generate
/// the event (XRCE-DDS, uORB; zenoh on bare metal), or `NROS_CPP_RET_FULL`
/// when the process-wide event-slot table (16 registrations) is full.
///
/// # Safety
/// `storage` must be a live poll-subscription storage; `user_context` must
/// outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_liveliness_changed(
    storage: *mut c_void,
    cb: nros_cpp_liveliness_changed_cb_t,
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
    unsafe {
        subscription_set_slot(
            storage,
            nros_rmw::EventKind::LivelinessChanged,
            0,
            cb,
            user_context,
        )
    }
}

/// Register a requested-deadline-missed callback — rclcpp's
/// `SubscriptionEventCallbacks::deadline_callback`, issue 1993. `deadline_ms`
/// reaches the backend's registration (zenoh uses it when the QoS states no
/// deadline; Cyclone DDS takes the QoS deadline). Same returns as
/// [`nros_cpp_subscription_set_liveliness_changed`].
///
/// # Safety
/// See [`nros_cpp_subscription_set_liveliness_changed`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_requested_deadline_missed(
    storage: *mut c_void,
    deadline_ms: u32,
    cb: nros_cpp_subscriber_count_cb_t,
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
    unsafe {
        subscription_set_slot(
            storage,
            nros_rmw::EventKind::RequestedDeadlineMissed,
            deadline_ms,
            cb,
            user_context,
        )
    }
}

/// Register a message-lost callback — rclcpp's
/// `SubscriptionEventCallbacks::message_lost_callback`, issue 1993. Same
/// returns as [`nros_cpp_subscription_set_liveliness_changed`].
///
/// # Safety
/// See [`nros_cpp_subscription_set_liveliness_changed`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_message_lost(
    storage: *mut c_void,
    cb: nros_cpp_subscriber_count_cb_t,
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
    unsafe {
        subscription_set_slot(
            storage,
            nros_rmw::EventKind::MessageLost,
            0,
            cb,
            user_context,
        )
    }
}

unsafe fn subscription_set_slot(
    storage: *mut c_void,
    kind: nros_rmw::EventKind,
    deadline_ms: u32,
    cb: *const (),
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() || cb.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let handle = storage as *mut nros::internals::RmwSubscriber;
    // Issue 1993 — the backend decides whether it carries `kind`.
    if !SubscriberTrait::supports_event(unsafe { &*handle }, kind) {
        return crate::NROS_CPP_RET_UNSUPPORTED;
    }
    let r = unsafe {
        nros_c::status_event_slots::register(
            storage as usize,
            storage,
            kind,
            cb,
            user_context,
            |k, tramp, tag| {
                SubscriberTrait::register_event_callback(&mut *handle, k, deadline_ms, tramp, tag)
            },
        )
    };
    match r {
        Ok(()) => NROS_CPP_RET_OK,
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Register a matched callback on a poll-style subscription — rclcpp's
/// `SubscriptionEventCallbacks::matched_callback`, issue 1972. Observed as the
/// subscription is polled (`has_data` / take).
///
/// # Safety
/// `storage` must be a live poll-subscription storage; `user_context` must
/// outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_matched(
    storage: *mut c_void,
    cb: crate::publisher::nros_cpp_matched_cb_t,
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
    unsafe {
        subscription_set_slot(
            storage,
            nros_rmw::EventKind::SubscriptionMatched,
            0,
            cb,
            user_context,
        )
    }
}

/// Register an incompatible-type callback on a poll-style subscription —
/// rclcpp's `SubscriptionEventCallbacks::incompatible_type_callback`.
///
/// # Safety
/// See [`nros_cpp_subscription_set_matched`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_incompatible_type(
    storage: *mut c_void,
    cb: crate::publisher::nros_cpp_incompatible_type_cb_t,
    user_context: *mut c_void,
) -> nros_cpp_ret_t {
    let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
    unsafe {
        subscription_set_slot(
            storage,
            nros_rmw::EventKind::SubscriptionIncompatibleType,
            0,
            cb,
            user_context,
        )
    }
}

/// The QoS profile this subscription is ACTUALLY running — issue 1437.
///
/// `rmw_subscription_get_actual_qos`. See
/// [`nros_cpp_publisher_get_actual_qos`](crate::publisher::nros_cpp_publisher_get_actual_qos)
/// for what "actual" means and why an unreportable policy reads back as that
/// enum's `UNKNOWN` rather than as the request.
///
/// **Two roads, one entry point.** A poll-style `rclcpp::Subscription<M>` owns
/// its subscriber in `storage`; a callback-style one does NOT — the executor
/// arena owns it and the C++ object holds only `(executor, handle_id)`. The
/// callback form is what a ported `rclcpp` node writes, so an accessor that
/// served only the first would be absent exactly where it is used. `storage`
/// non-NULL selects the first; otherwise `(executor, handle_id)` is resolved.
///
/// # Safety
/// Exactly one of `storage` / `executor` identifies a live subscription;
/// `out_qos` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_get_actual_qos(
    storage: *const c_void,
    executor: *mut c_void,
    handle_id: usize,
    out_qos: *mut nros_cpp_qos_t,
) -> nros_cpp_ret_t {
    if out_qos.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let granted = if !storage.is_null() {
        let subscriber = unsafe { &*(storage as *const nros::internals::RmwSubscriber) };
        SubscriberTrait::actual_qos(subscriber)
    } else {
        let Some(ctx) = (unsafe { crate::cpp_ctx_checked(executor) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        match unsafe { ctx.executor.subscription_handle(handle_id) } {
            Some(handle) => SubscriberTrait::actual_qos(handle),
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };
    unsafe {
        *out_qos = nros_cpp_qos_t::from_qos_settings(granted);
    }
    NROS_CPP_RET_OK
}

#[cfg(test)]
mod tests {
    //! phase-460 W7 / issue 1425 -- the C++ take counts and NAMES a sample it
    //! threw away for being too big.
    //!
    //! WHAT THIS TEST IS, AND WHAT THE PHASE DOC ASKED FOR. The wave's gate is
    //! written as "register a C++ subscription with a 16-byte buffer, publish a
    //! 64-byte sample". That cannot be run here, and the reason is structural
    //! rather than effort: `nros_cpp_subscription_create` needs a REGISTERED
    //! RMW backend, and a `cargo test -p nros-cpp` binary has none -- a backend
    //! self-registers from `.init_array` in an image that links one, and this
    //! lane links none. So the drop is raised at the seam the take path reaches
    //! once the backend has already discarded the sample, with the same buffer
    //! size, and the ASSERTIONS are the ones the gate names: the counter moved
    //! by exactly one, and the line a reader sees carries the number they have
    //! to act on.
    //!
    //! The sample size is issue 1612. A too-small `take` now carries the size
    //! the sample needed in the span's `len`; each backend's own suite proves
    //! it writes that number, `nros-rmw-cffi`'s `refused_take_len.rs` proves
    //! it reaches [`SubscriberTrait::refused_sample_len`], and the cells below
    //! prove the line prints it beside the buffer -- or `?` when the backend
    //! could not know.

    use super::*;
    use core::sync::atomic::Ordering;

    /// A sink that keeps what was logged, so the test reads what a human
    /// would see rather than what the format string says. `&'static` because
    /// `add_sink` takes one and the dispatcher holds it for the process.
    struct CapturingSink {
        lines: std::sync::Mutex<std::vec::Vec<std::string::String>>,
    }

    impl nros_log::LogSink for CapturingSink {
        fn log(&self, record: &nros_log::Record<'_>) {
            self.lines
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(record.message.into());
        }
    }

    static CAPTURE: std::sync::OnceLock<&'static CapturingSink> = std::sync::OnceLock::new();

    /// The counter, the sink and the throttle are all PROCESS-global, so the
    /// cells below must not interleave.
    ///
    /// `cargo test` runs them as threads in one process; nextest gives each its
    /// own. Only one of those two needs this lock, and which lane runs the
    /// tests is not something a test should have to know. Poisoning is ignored
    /// deliberately: a panic in one cell must fail that cell, not turn the
    /// other two into a second, more confusing failure.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Serialise, then put the process-global state back to zero.
    ///
    /// The THROTTLE is why this resets rather than measuring a delta: "the
    /// first drop is always reported" is a statement about drop number zero,
    /// and a cell that inherited another cell's count would be asserting
    /// something else entirely -- which is exactly how this test first failed.
    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let guard = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sink = CAPTURE.get_or_init(|| {
            let sink: &'static CapturingSink =
                std::boxed::Box::leak(std::boxed::Box::new(CapturingSink {
                    lines: std::sync::Mutex::new(std::vec::Vec::new()),
                }));
            assert!(
                nros_log::add_sink(sink),
                "no sink slot left; this test cannot see what a reader would"
            );
            sink
        });
        SAMPLES_DROPPED_TOO_SMALL.store(0, Ordering::Relaxed);
        sink.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        guard
    }

    fn logged() -> std::vec::Vec<std::string::String> {
        let sink = CAPTURE.get().expect("`fresh()` installs the sink");
        let mut guard = sink
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        core::mem::take(&mut *guard)
    }

    /// Raise drop number `nth` (1-based) until its line reaches the sink.
    ///
    /// `nros_log` holds a record raised while another thread is delivering and
    /// DROPS one that does not fit (issue 2000), and other tests in this
    /// binary log from their own threads, so a line asserted on can be lost
    /// outright. `nros_log::early::emit_until_delivered` re-raises it; the
    /// counter is set inside the emit so every raise is drop number `nth`
    /// again. A re-raise can deliver the line twice, so callers use
    /// [`distinct`].
    fn raise_drop_until_logged(nth: u32, raise: impl Fn()) -> bool {
        let sink = CAPTURE.get().expect("`fresh()` installs the sink");
        let total = std::format!("{nth} dropped so far");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        nros_log::early::emit_until_delivered(
            || {
                SAMPLES_DROPPED_TOO_SMALL.store(nth - 1, Ordering::Relaxed);
                raise();
            },
            || {
                sink.lines
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .any(|l| l.contains(&total))
            },
            || {
                std::thread::yield_now();
                std::time::Instant::now() < deadline
            },
        )
    }

    /// The distinct lines, in first-seen order: a re-raised record is the same
    /// drop again, never a second one.
    fn distinct(lines: std::vec::Vec<std::string::String>) -> std::vec::Vec<std::string::String> {
        let mut out: std::vec::Vec<std::string::String> = std::vec::Vec::new();
        for l in lines {
            if !out.contains(&l) {
                out.push(l);
            }
        }
        out
    }

    /// THE GATE. A 16-byte buffer, a sample that did not fit: counted once, and
    /// the line names the buffer, the knob and which drop it was.
    #[test]
    fn a_too_small_take_is_counted_once_and_named_in_the_log() {
        let _serial = fresh();

        let seen = raise_drop_until_logged(1, || {
            note_sample_dropped_too_small(&TransportError::BufferTooSmall, 16, Some(1000))
        });

        assert_eq!(
            SAMPLES_DROPPED_TOO_SMALL.load(Ordering::Relaxed),
            1,
            "the drop was not counted; NROS_CPP_RET_FULL with out_len 0 is all \
             the caller would have seen, and that is what an idle topic looks \
             like"
        );

        let lines = distinct(logged());
        assert!(
            seen && lines.len() == 1,
            "the FIRST drop must always be reported; got {lines:?}"
        );
        let line = &lines[0];
        assert!(
            line.contains("16-byte buffer"),
            "the line does not name the buffer size, which is the number to \
             act on: {line}"
        );
        // Issue 1612 -- BOTH sizes: the one to raise the knob to, beside the
        // one it is now.
        assert!(
            line.contains("1000-byte sample"),
            "the line does not name the sample's size, which is what the knob \
             must reach: {line}"
        );
        // `nros_log`'s push is all-or-nothing into 256 bytes, so a line that
        // grew past it loses its tail whole; the issue number is the tail.
        assert!(
            line.contains("(issue 1425)."),
            "the line was truncated -- it no longer fits nros_log's buffer: {line}"
        );
        assert!(
            line.contains("NROS_SUBSCRIPTION_BUFFER_SIZE"),
            "the line does not name the knob to raise: {line}"
        );
        assert!(
            line.contains("BufferTooSmall"),
            "the line does not say WHICH drop this was: {line}"
        );
    }

    /// Issue 1612 -- a backend that cannot know the sample's size says so,
    /// rather than printing a number nobody measured.
    #[test]
    fn an_unknown_sample_size_is_printed_as_unknown() {
        let _serial = fresh();
        let seen = raise_drop_until_logged(1, || {
            note_sample_dropped_too_small(&TransportError::BufferTooSmall, 64, None)
        });
        let lines = distinct(logged());
        assert!(seen && lines.len() == 1, "got {lines:?}");
        assert!(
            lines[0].contains("?-byte sample") && lines[0].contains("64-byte buffer"),
            "an unknown size must read as unknown beside the known buffer: {}",
            lines[0]
        );
    }

    /// The throttle: the first, then every 64th. A forty-participant graph must
    /// not turn one misconfigured subscription into a log flood (issue 0371's
    /// shape), and a throttle nobody measured is a claim.
    #[test]
    fn the_drop_log_is_throttled_but_never_silent() {
        let _serial = fresh();
        let drop = || note_sample_dropped_too_small(&TransportError::MessageTooLarge, 32, None);
        // The two that log are raised until seen; the 63 between them are
        // the throttle's to silence.
        let first = raise_drop_until_logged(1, drop);
        for _ in 1..64 {
            drop();
        }
        let last = raise_drop_until_logged(65, drop);
        let lines = distinct(logged());
        assert!(first && last, "a logged drop never arrived: {lines:?}");
        assert_eq!(
            lines.len(),
            2,
            "65 drops must log the 1st and the 65th and nothing between; got \
             {lines:?}"
        );
        assert!(
            lines[1].contains("65"),
            "the throttled line does not carry the RUNNING TOTAL, which is the \
             only thing that says how much was lost between lines: {}",
            lines[1]
        );
    }

    /// Every drop reaches the RECORD, throttled or not.
    ///
    /// This is the half that matters on the island. The log line is the thing
    /// that board cannot deliver, so a tally that followed the throttle would
    /// leave a console-less image reading 64 times short -- and the record is
    /// the only channel left.
    #[test]
    fn the_tally_counts_every_drop_not_only_the_logged_ones() {
        let _serial = fresh();
        let drop = || note_sample_dropped_too_small(&TransportError::BufferTooSmall, 8, Some(9));
        let seen = raise_drop_until_logged(1, drop);
        for _ in 1..10 {
            drop();
        }
        assert_eq!(
            SAMPLES_DROPPED_TOO_SMALL.load(Ordering::Relaxed),
            10,
            "the tally followed the throttle"
        );
        let lines = distinct(logged());
        assert!(seen && lines.len() == 1, "and the log did not: {lines:?}");
    }
}
