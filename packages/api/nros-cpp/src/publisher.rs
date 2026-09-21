//! Publisher FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): the caller's opaque storage holds the
//! `RmwPublisher` handle -- no `CppPublisher` wrapper bundling topic-name
//! metadata. The `nros::Publisher<M>` C++ class owns the topic name buffer
//! alongside the storage.
//!
//! phase-462 W1 (RFC-0052): the storage is [`CppPublisher`] again, but it is
//! the Rust handle's own shape, not metadata: `EmbeddedPublisher` carries its
//! contracted endpoint's `PubMonitorCell` beside the RMW handle and bumps it
//! on every publish, and this is the only place a C++ publish passes through
//! (every typed `M::ffi_publish` lands in `nros_cpp_publish_raw`). The cell
//! is resolved at create time by exact topic match against the executor's
//! installed table (`nros_cpp_install_monitors`), the same rule
//! `NodeHandle::create_publisher` applies; an uncontracted publisher carries a
//! null and pays one null test per publish.

use core::ffi::{c_char, c_void};

use nros_rmw::{Publisher as PublisherTrait, Session, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK, cstr_to_str,
    nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// What the caller's `NROS_PUBLISHER_SIZE + sizeof(void*)` bytes hold: the
/// RMW handle and the contracted endpoint's counter cell (null when the topic
/// has no row in the installed monitor table).
///
/// `#[repr(C)]` so the size is the handle plus exactly one pointer, which is
/// what `nros/publisher.hpp` reserves; the assert below is the contract.
#[repr(C)]
pub(crate) struct CppPublisher {
    pub(crate) handle: nros::internals::RmwPublisher,
    pub(crate) monitor: *const nros::monitor::PubMonitorCell,
}

const _: () = {
    use core::mem::{align_of, size_of};
    assert!(
        size_of::<CppPublisher>()
            == size_of::<nros::internals::RmwPublisher>() + size_of::<*const c_void>()
    );
    // `nros/publisher.hpp` declares the storage `alignas(8)`.
    assert!(align_of::<CppPublisher>() <= 8);
};

impl CppPublisher {
    /// RFC-0052 W3b.4 -- one relaxed bump per publish on a contracted
    /// endpoint; a null test otherwise. The mirror of
    /// `EmbeddedPublisher::bump_monitor`.
    #[inline]
    fn bump_monitor(&self) {
        if let Some(cell) = unsafe { self.monitor.as_ref() } {
            cell.count
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Create a publisher on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<CppPublisher>()` bytes (`NROS_PUBLISHER_SIZE` from the
/// generated header plus one pointer for the monitor cell), 8-aligned. The
/// handle and the cell pointer are written directly into this buffer.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-aligned buffer of at least `NROS_PUBLISHER_SIZE + sizeof(void*)` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_create(
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

    // Extract node name/namespace from the node handle
    let node_name_str = core::str::from_utf8(&node_ref.name)
        .ok()
        .and_then(|s| s.split('\0').next());
    let ns_str = core::str::from_utf8(&node_ref.namespace)
        .ok()
        .and_then(|s| s.split('\0').next())
        .unwrap_or("/");

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Phase 305 W3 (issue 0255) — expand `~`/relative topic names against this
    // node's identity and apply its launch remap rules (executor-side table)
    // before the name reaches the wire. QoS overrides below keep matching the
    // SOURCE spelling (the plan writes them against launch names).
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
    // publisher role, mirroring Rust's `create_publisher_with_qos`.
    let qos_settings = unsafe {
        crate::apply_qos_overrides(
            qos.to_qos_settings(),
            node_ref.qos_overrides,
            node_ref.qos_overrides_len,
            topic_str,
            crate::NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER,
        )
    };

    // Phase 104.C.9.b — route through the Node's session when the
    // Node was bound to a non-primary RMW backend via
    // `nros_cpp_node_create_ex`.
    let session = match crate::node_id_opt(node_ref) {
        Some(id) => match ctx.executor.node_session_mut(id) {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        },
        None => ctx.executor.session_mut(),
    };

    match session.create_publisher(&topic_info, qos_settings) {
        Ok(handle) => {
            // phase-462 W1 -- attach the contracted endpoint's counter cell by
            // exact match on the SOURCE topic spelling, the rule
            // `NodeHandle::create_publisher` applies (the model's wiring
            // carries the same name). Null when the table has no row, which
            // is every publisher of an uncontracted image.
            let monitor = ctx
                .executor
                .monitor_table()
                .iter()
                .find(|m| m.topic == topic_str)
                .map_or(core::ptr::null(), |m| m.cell as *const _);
            unsafe {
                core::ptr::write(
                    storage as *mut CppPublisher,
                    CppPublisher { handle, monitor },
                );
            }
            NROS_CPP_RET_OK
        }
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Publish raw CDR data.
///
/// # Safety
/// `storage` must be a valid publisher storage (initialised by
/// `nros_cpp_publisher_create`). `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publish_raw(
    storage: *mut c_void,
    data: *const u8,
    len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let publisher = unsafe { &*(storage as *const CppPublisher) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    publisher.bump_monitor();
    match publisher.handle.publish_raw(data_slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 124.E.1 — streamed publish.
///
/// Two callbacks: `size_cb` reports total payload length once,
/// `chunk_cb` fills the slot in chunks. Backends that support
/// streaming land each chunk directly in their outbound buffer;
/// backends that don't fall through to a stack staging buffer
/// (capped at ~4 KiB) + a single `publish_raw`.
///
/// # Safety
/// `storage` must be a valid publisher. The callbacks MUST NOT
/// outlive the call; `user_ctx` is valid only for the duration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_publish_streamed(
    storage: *mut c_void,
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
) -> nros_cpp_ret_t {
    use nros_rmw::Publisher;
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let size_cb = match size_cb {
        Some(f) => f,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let chunk_cb = match chunk_cb {
        Some(f) => f,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    publisher.bump_monitor();
    // SAFETY: this C++ FFI entry point is unsafe; callers must keep
    // `user_ctx` valid for the synchronous callback sequence.
    match unsafe {
        publisher
            .handle
            .publish_streamed(size_cb, chunk_cb, user_ctx)
    } {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

// ============================================================================
// Phase 124.A.7 — zero-copy publisher loan / commit / discard
// ============================================================================

/// Phase 124.A.7 — loan a writable slot of `requested_len` bytes from
/// the publisher's outbound buffer.
///
/// # AVAILABILITY — read this before calling
///
/// This symbol exists **only** in a nano-ros built with the `lending`
/// cargo feature, and **no shipped configuration enables it**: the
/// string `lending` appears zero times under `cmake/` and `zephyr/`,
/// `nros-rmw-zenoh-staticlib` declares no forwarder for it, and the only
/// crate in the tree that turns it on is the `nros-tests` harness.
///
/// Its presence in this header is therefore **not evidence that it can
/// be linked.** cbindgen emits every declaration unconditionally here,
/// by deliberate repo-wide policy, so a `#[cfg]`-gated Rust symbol still
/// appears. `nros/publisher.hpp`'s `Publisher::loan()` calls it with no
/// guard of its own, so against a default build the failure surfaces at
/// LINK as an undefined symbol, not at the call site.
///
/// The supported zero-copy surface is `publish_streamed` and its receive
/// twin `process_raw_in_place`: no feature, no token, no arena, no size
/// ceiling. See issue 0814.
///
/// On success, `*out_buf` points at `*out_cap` writable bytes the
/// caller fills in place. Pass `*out_token` back to
/// `nros_cpp_publisher_commit` (to send) or
/// `nros_cpp_publisher_discard` (to abandon).
///
/// # Safety
/// All pointer parameters must be valid. `storage` must be an initialized
/// publisher handle. The token persists across FFI calls; caller MUST
/// commit OR discard before the publisher is destroyed.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_loan(
    storage: *mut c_void,
    requested_len: usize,
    out_buf: *mut *mut u8,
    out_cap: *mut usize,
    out_token: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null()
        || out_buf.is_null()
        || out_cap.is_null()
        || out_token.is_null()
        || requested_len == 0
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // Issue 0812 — `try_lend_raw` hands back the BACKEND's own token, so
    // nothing has to be stored on this side of the FFI boundary. This used
    // to `Box` a lifetime-erased slot per loan: a malloc on the path whose
    // entire purpose is removing copies.
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    match publisher.handle.try_lend_raw(requested_len) {
        Ok(Some((buf_ptr, cap, token))) => {
            unsafe {
                *out_buf = buf_ptr;
                *out_cap = cap;
                *out_token = token;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => crate::NROS_CPP_RET_TRY_AGAIN,
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Phase 124.A.7 — commit a previously loaned slot.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `nros_cpp_publisher_loan`. Issue 0814.
///
/// # Safety
/// `storage` must be the publisher the token was loaned from. `token`
/// must come from a matching `nros_cpp_publisher_loan` and must not be
/// reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_commit(
    storage: *mut c_void,
    token: *mut c_void,
    actual_len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    publisher.bump_monitor();
    // SAFETY: `token` is the backend token a prior `nros_cpp_publisher_loan`
    // handed out on this publisher; the caller's contract is that it is
    // still outstanding and is consumed here exactly once.
    match unsafe { publisher.handle.commit_raw(token, actual_len) } {
        Ok(()) => NROS_CPP_RET_OK,
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Phase 124.A.7 — abandon a previously loaned slot.
///
/// **Availability:** `lending`-only, and no shipped build enables it —
/// see `nros_cpp_publisher_loan`. Issue 0814.
///
/// # Safety
/// `storage` must be the publisher the token was loaned from. `token`
/// must come from a matching `nros_cpp_publisher_loan` and must not be
/// reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_discard(
    storage: *mut c_void,
    token: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() || token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    // SAFETY: `token` is the backend token a prior `nros_cpp_publisher_loan`
    // handed out on this publisher. `discard_raw` fires the backend's
    // pub_discard (or reclaims the arena staging buffer) — issue 0812
    // retired the per-loan Box this used to reconstitute.
    match unsafe { publisher.handle.discard_raw(token) } {
        Ok(()) => NROS_CPP_RET_OK,
        Err(e) => crate::transport_error_to_cpp_ret(e),
    }
}

/// Destroy a publisher (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized publisher storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut CppPublisher);
    }
    NROS_CPP_RET_OK
}

/// Relocate a publisher from `old_storage` to `new_storage`.
///
/// Neither the `RmwPublisher` nor the monitor cell pointer beside it
/// references the storage address, so relocation is a straight `ptr::read`
/// + `ptr::write` (the cell itself is a static the executor owns). Called
/// by the C++ `Publisher` move ctor / move assignment.
///
/// # Safety
/// Both `old_storage` and `new_storage` must be valid, 8-aligned buffers of
/// at least `NROS_PUBLISHER_SIZE + sizeof(void*)` bytes. `old_storage` must
/// contain an initialised publisher; `new_storage` must not. After the
/// call, `old_storage` is logically uninitialised and must not be destroyed
/// — the C++ side sets its `initialized_` flag to `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut CppPublisher);
        core::ptr::write(new_storage as *mut CppPublisher, value);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 108 — publisher-side status events (stub: NROS_CPP_RET_UNSUPPORTED)
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_pub_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

pub type nros_cpp_publisher_count_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_pub_count_status_t,
        user_context: *mut c_void,
    ),
>;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_set_liveliness_lost(
    _storage: *mut c_void,
    _cb: nros_cpp_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_set_offered_deadline_missed(
    _storage: *mut c_void,
    _deadline_ms: u32,
    _cb: nros_cpp_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

/// Phase 108.B.7 — manually assert this publisher's liveliness.
///
/// Required for entities created with QoS `liveliness_kind =
/// MANUAL_BY_TOPIC` / `MANUAL_BY_NODE`. No-op otherwise. Backends
/// without manual-assertion wiring return `OK` (the trait default).
///
/// # Safety
/// `storage` must be a valid publisher storage (initialised by
/// `nros_cpp_publisher_create`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_assert_liveliness(
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    match publisher.handle.assert_liveliness() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}
