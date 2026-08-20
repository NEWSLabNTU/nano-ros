//! Phase 115.A.2 — smoke test for `nros_rmw_cffi_set_custom_transport`.
//!
//! Covers:
//!  - happy path (V1 ops install + clear via NULL)
//!  - abi_version mismatch -> NROS_RMW_RET_INCOMPATIBLE_ABI
//!  - install does NOT clobber on rejection
//!  - a NULL callback slot is rejected, not transmuted into an invalid `fn`
//!    (issue 0331)
//!
//! No backend involved — the test interacts directly with
//! `nros_rmw_cffi`'s C ABI surface, which is what a non-Rust
//! consumer would do.

// issue 0724 — link the host platform port into THIS test binary.
//
// The `nros-platform-cffi[posix-c-port]` dev-dependency defines
// `nros_platform_log_write` / `_flush`, which `nros-log`'s `PlatformSink` calls
// and issue 0710 made mandatory for anything that logs. rustc DROPS an
// `--extern` nothing references, and with it the build script's `link-lib`, so
// the dependency alone leaves the symbols undefined. `nros-node` and
// `nros-tests` carry the same line for the same reason.
//
// Per test binary because each integration test is its own crate. It cannot
// move into the lib: an unconditional anchor there would force the POSIX port
// on every `no_std` consumer. In ALL of them rather than only the nine that log
// today — one spelling, no list of which tests are allowed to log.
extern crate nros_platform_cffi as _;

use core::ffi::c_void;

use nros_rmw::{NROS_TRANSPORT_OPS_ABI_VERSION_V1, peek_custom_transport, take_custom_transport};
use nros_rmw_cffi::{
    NROS_RMW_RET_INCOMPATIBLE_ABI, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, generated,
    nros_rmw_cffi_set_custom_transport,
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

/// issue 0331 — build the GENERATED type, which is what a C caller actually
/// passes. Its fn slots are `Option<fn>` because C pointers are nullable; the
/// export used to take the hand-written Rust mirror, whose slots are plain
/// `fn`, so this asymmetry was invisible from the test side.
fn make_ops() -> generated::nros_transport_ops_t {
    generated::nros_transport_ops_t {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: core::ptr::null_mut(),
        open: Some(stub_open),
        close: Some(stub_close),
        write: Some(stub_write),
        read: Some(stub_read),
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

/// A NULL slot is what the null-pointer optimization would silently turn into
/// an invalid `fn` — calling it is UB. It must be refused at the boundary, and
/// like an ABI mismatch it must not clobber an existing install.
#[test]
fn null_callback_slot_rejected() {
    let good = make_ops();
    let rc = unsafe { nros_rmw_cffi_set_custom_transport(&good) };
    assert_eq!(rc, NROS_RMW_RET_OK);

    for (name, mut bad) in [
        ("open", make_ops()),
        ("close", make_ops()),
        ("write", make_ops()),
        ("read", make_ops()),
    ] {
        match name {
            "open" => bad.open = None,
            "close" => bad.close = None,
            "write" => bad.write = None,
            _ => bad.read = None,
        }
        let rc = unsafe { nros_rmw_cffi_set_custom_transport(&bad) };
        assert_eq!(
            rc, NROS_RMW_RET_INVALID_ARGUMENT,
            "NULL `{name}` must be refused"
        );
        assert!(
            peek_custom_transport().is_some(),
            "refusing NULL `{name}` must not clobber the previous install"
        );
    }

    let _ = take_custom_transport();
}
