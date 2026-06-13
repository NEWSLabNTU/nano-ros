//! Publisher FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): the caller's opaque storage now holds
//! a bare `RmwPublisher` handle — no more `CppPublisher` wrapper bundling
//! topic-name metadata. The `nros::Publisher<M>` C++ class owns the topic
//! name buffer alongside the storage.

use core::ffi::{c_char, c_void};

use nros_rmw::{Publisher as PublisherTrait, Session, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Create a publisher on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwPublisher>()` bytes (exposed via `NROS_PUBLISHER_SIZE` in
/// the generated header), aligned to its alignment requirement. The
/// `RmwPublisher` handle is written directly into this buffer.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// appropriately-aligned buffer of at least `NROS_PUBLISHER_SIZE` bytes.
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
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => s,
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

    let topic_info = TopicInfo::new(topic_str, type_str, hash_str)
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
    let session = if node_ref.node_id != 0 {
        match ctx
            .executor
            .node_session_mut(nros_node::executor::NodeId::from_raw(node_ref.node_id))
        {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    } else {
        ctx.executor.session_mut()
    };

    match session.create_publisher(&topic_info, qos_settings) {
        Ok(handle) => {
            // Write the bare RmwPublisher handle into caller-provided storage.
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwPublisher, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
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

    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match publisher.publish_raw(data_slice) {
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
    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    // SAFETY: this C++ FFI entry point is unsafe; callers must keep
    // `user_ctx` valid for the synchronous callback sequence.
    match unsafe { publisher.publish_streamed(size_cb, chunk_cb, user_ctx) } {
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

    use nros_rmw::SlotLending;
    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    match publisher.try_lend_slot(requested_len) {
        Ok(Some(slot)) => {
            // SAFETY: erase lifetime — caller's contract ensures commit
            // / discard happens before publisher destruction.
            let mut slot: nros::internals::RmwSlot<'static> = unsafe {
                core::mem::transmute::<
                    nros::internals::RmwSlot<'_>,
                    nros::internals::RmwSlot<'static>,
                >(slot)
            };
            let buf_ptr = slot.as_mut().as_mut_ptr();
            let cap = slot.as_mut().len();
            let boxed = alloc::boxed::Box::new(slot);
            unsafe {
                *out_buf = buf_ptr;
                *out_cap = cap;
                *out_token = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => crate::NROS_CPP_RET_TRY_AGAIN,
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Phase 124.A.7 — commit a previously loaned slot.
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
    use nros_rmw::SlotLending;
    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    let mut slot: alloc::boxed::Box<nros::internals::RmwSlot<'static>> =
        unsafe { alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwSlot<'static>) };
    slot.set_len(actual_len);
    match publisher.commit_slot(*slot) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Phase 124.A.7 — abandon a previously loaned slot.
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
    let _slot: alloc::boxed::Box<nros::internals::RmwSlot<'static>> =
        unsafe { alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwSlot<'static>) };
    NROS_CPP_RET_OK
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
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwPublisher);
    }
    NROS_CPP_RET_OK
}

/// Relocate an `RmwPublisher` from `old_storage` to `new_storage`.
///
/// `RmwPublisher` registers nothing externally that references its storage
/// address, so relocation is a straight `ptr::read` + `ptr::write`. Called
/// by the C++ `Publisher` move ctor / move assignment.
///
/// # Safety
/// Both `old_storage` and `new_storage` must be valid, appropriately-aligned
/// buffers of at least `NROS_PUBLISHER_SIZE` bytes. `old_storage` must
/// contain an initialised `RmwPublisher`; `new_storage` must not. After the
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
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwPublisher);
        core::ptr::write(new_storage as *mut nros::internals::RmwPublisher, value);
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
    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    match publisher.assert_liveliness() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}
