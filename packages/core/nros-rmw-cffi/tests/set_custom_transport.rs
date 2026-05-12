//! Phase 115.A.2 — smoke test for `nros_rmw_cffi_set_custom_transport`.
//!
//! Covers:
//!  - happy path (V1 ops install + clear via NULL)
//!  - abi_version mismatch -> NROS_RMW_RET_INCOMPATIBLE_ABI
//!  - install does NOT clobber on rejection
//!
//! No backend involved — the test interacts directly with
//! `nros_rmw_cffi`'s C ABI surface, which is what a non-Rust
//! consumer would do.

use core::ffi::c_void;

use nros_rmw::{
    NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, peek_custom_transport,
    take_custom_transport,
};
use nros_rmw_cffi::{
    NROS_RMW_RET_INCOMPATIBLE_ABI, NROS_RMW_RET_OK, nros_rmw_cffi_set_custom_transport,
};

unsafe extern "C" fn stub_open(_user: *mut c_void, _params: *const c_void) -> i32 {
    0
}
unsafe extern "C" fn stub_close(_user: *mut c_void) {}
unsafe extern "C" fn stub_write(_user: *mut c_void, _buf: *const u8, _len: usize) -> i32 {
    0
}
unsafe extern "C" fn stub_read(
    _user: *mut c_void,
    _buf: *mut u8,
    _len: usize,
    _timeout_ms: u32,
) -> i32 {
    0
}

fn make_ops() -> NrosTransportOps {
    NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: core::ptr::null_mut(),
        open: stub_open,
        close: stub_close,
        write: stub_write,
        read: stub_read,
    }
}

#[test]
fn install_v1_then_clear() {
    // Pre: empty slot.
    let _ = take_custom_transport();

    let ops = make_ops();
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(&ops) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(peek_custom_transport().is_some());

    // Clear via NULL.
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(core::ptr::null()) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(peek_custom_transport().is_none());
}

#[test]
fn abi_version_mismatch_rejected() {
    // Pre: empty slot.
    let _ = take_custom_transport();

    let mut ops = make_ops();
    ops.abi_version = 0xDEAD_BEEF;
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(&ops) };
    assert_eq!(rc, NROS_RMW_RET_INCOMPATIBLE_ABI);
    // Slot stays empty after the rejected call.
    assert!(peek_custom_transport().is_none());
}

#[test]
fn rejection_preserves_previous_install() {
    // Install a valid one first.
    let good = make_ops();
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(&good) };
    assert_eq!(rc, NROS_RMW_RET_OK);

    // Bad install must NOT clobber.
    let mut bad = make_ops();
    bad.abi_version = 0xBAD0_BAD0;
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(&bad) };
    assert_eq!(rc, NROS_RMW_RET_INCOMPATIBLE_ABI);
    assert!(peek_custom_transport().is_some());

    // Clean up.
    let _ = take_custom_transport();
}
