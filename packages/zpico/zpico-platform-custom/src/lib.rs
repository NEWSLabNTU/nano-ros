//! Phase 115.B — Rust drain symbol for zenoh-pico's custom-link
//! scheme.
//!
//! The zenoh-pico C source `src/link/unicast/custom.c` (in our
//! vendored fork at
//! `packages/zpico/zpico-sys/zenoh-pico/src/link/unicast/custom.c`)
//! calls `nros_zpico_custom_take(out)` from inside
//! `_z_f_link_open_custom`. This crate is the Rust side of that
//! symbol — it drains the slot populated by
//! `nros_rmw::set_custom_transport(...)` (Phase 115.A.2) into the
//! caller-provided struct.
//!
//! The C side embeds a *copy* of the vtable into its `_z_link_t`
//! socket struct, so once `take` returns the slot is empty and a
//! follow-up registration won't disturb the running session.
//!
//! # Crate purpose
//!
//! This crate exists ONLY to expose the `nros_zpico_custom_take`
//! symbol. We can't put it directly in `zpico-sys` because that
//! crate doesn't depend on `nros-rmw`, and adding the dep would
//! cycle through the broader RMW build graph. We can't put it in
//! `nros-rmw-zenoh` because the C custom-link source files live
//! in `zpico-sys`'s C tree and link before `nros-rmw-zenoh` is
//! built. A standalone tiny crate breaks the dep cycle cleanly.

#![no_std]

use core::ffi::c_void;

/// Phase 115.B — C-ABI mirror of `nros_rmw::NrosTransportOps` so
/// the C custom-link source can refer to a struct we own. Same
/// `#[repr(C)]` layout — single canonical ABI.
///
/// This struct is **also** declared in zenoh-pico's
/// `include/zenoh-pico/system/link/custom.h` as
/// `_z_custom_ops_t`. The Rust + C definitions must remain
/// byte-compatible; the layout-drift compile-time assertion below
/// catches the obvious cases.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ZpicoCustomOps {
    pub abi_version: u32,
    pub _reserved: u32,
    pub user_data: *mut c_void,
    pub open: unsafe extern "C" fn(user_data: *mut c_void, params: *const c_void) -> i32,
    pub close: unsafe extern "C" fn(user_data: *mut c_void),
    pub write: unsafe extern "C" fn(user_data: *mut c_void, buf: *const u8, len: usize) -> i32,
    pub read: unsafe extern "C" fn(
        user_data: *mut c_void,
        buf: *mut u8,
        len: usize,
        timeout_ms: u32,
    ) -> i32,
}

const _: () = {
    assert!(
        core::mem::size_of::<ZpicoCustomOps>()
            == core::mem::size_of::<nros_rmw::NrosTransportOps>()
    );
    assert!(
        core::mem::align_of::<ZpicoCustomOps>()
            == core::mem::align_of::<nros_rmw::NrosTransportOps>()
    );
};

/// Drain the registered `NrosTransportOps` from
/// `nros_rmw::take_custom_transport()` into `out`.
///
/// # Returns
///
/// - `0` on success — `out` was populated and the slot is now
///   empty.
/// - `-1` if no transport was registered. The C side translates
///   this to `_Z_ERR_TRANSPORT_OPEN_FAILED` so session-open
///   fails cleanly.
///
/// # Safety
///
/// `out` must point to a writable, properly-aligned
/// `ZpicoCustomOps` (== `_z_custom_ops_t`). The C custom-link
/// factory always passes a struct embedded inside `_z_link_t`,
/// which is heap- or stack-allocated and lives for the link's
/// lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_zpico_custom_take(out: *mut ZpicoCustomOps) -> i32 {
    if out.is_null() {
        return -1;
    }
    let Some(ops) = nros_rmw::take_custom_transport() else {
        return -1;
    };
    let zpico_ops = ZpicoCustomOps {
        abi_version: ops.abi_version,
        _reserved: ops._reserved,
        user_data: ops.user_data,
        open: ops.open,
        close: ops.close,
        write: ops.write,
        read: ops.read,
    };
    unsafe { core::ptr::write(out, zpico_ops) };
    0
}
