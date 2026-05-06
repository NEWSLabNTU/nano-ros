//! Phase 115.E / 115.G — XRCE custom-transport bridge integration test.
//!
//! Verifies that `nros_rmw::set_custom_transport` + the XRCE-side
//! `init_transport_from_custom_ops` plumb the four user callbacks
//! into the C trampolines without actually opening an XRCE session
//! (which would need a running MicroXRCEAgent).
//!
//! Run via:
//! ```bash
//! cargo test -p nros-rmw-xrce --features platform-posix \
//!     --test custom_transport
//! ```

#![cfg(feature = "platform-posix")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

use nros_rmw::{NrosTransportOps, peek_custom_transport, set_custom_transport};

static OPEN_CALLS: AtomicU32 = AtomicU32::new(0);
static CLOSE_CALLS: AtomicU32 = AtomicU32::new(0);
static WRITE_CALLS: AtomicU32 = AtomicU32::new(0);
static READ_CALLS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn stub_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    OPEN_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn stub_close(_ud: *mut c_void) {
    CLOSE_CALLS.fetch_add(1, Ordering::Relaxed);
}
unsafe extern "C" fn stub_write(_ud: *mut c_void, _buf: *const u8, len: usize) -> i32 {
    WRITE_CALLS.fetch_add(1, Ordering::Relaxed);
    let _ = len;
    0
}
unsafe extern "C" fn stub_read(_ud: *mut c_void, _buf: *mut u8, _len: usize, _to: u32) -> i32 {
    READ_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}

fn make_ops() -> NrosTransportOps {
    NrosTransportOps {
        user_data: 0xDEAD_BEEF_usize as *mut c_void,
        open: stub_open,
        close: stub_close,
        write: stub_write,
        read: stub_read,
    }
}

#[test]
fn set_custom_transport_round_trips_through_xrce_bridge() {
    OPEN_CALLS.store(0, Ordering::Relaxed);
    CLOSE_CALLS.store(0, Ordering::Relaxed);
    WRITE_CALLS.store(0, Ordering::Relaxed);
    READ_CALLS.store(0, Ordering::Relaxed);

    // Register the vtable.
    unsafe { set_custom_transport(Some(make_ops())) };

    // Peek to confirm registration landed.
    let peeked = peek_custom_transport().expect("peek after register");
    assert_eq!(peeked.user_data as usize, 0xDEAD_BEEF);

    // Drain the slot via the XRCE bridge. This consumes the
    // registration, copies the four fn pointers + user_data into
    // XRCE-local trampoline state, and registers C trampolines with
    // `uxr_set_custom_transport_callbacks`.
    let ok = unsafe { nros_rmw_xrce::init_transport_from_custom_ops(false) };
    assert!(ok, "init_transport_from_custom_ops returned false");

    // After draining, the global slot is empty.
    assert!(peek_custom_transport().is_none());

    // Calling `init_transport_from_custom_ops` again w/o re-registering
    // returns false — nothing to install.
    let ok2 = unsafe { nros_rmw_xrce::init_transport_from_custom_ops(false) };
    assert!(!ok2, "second call should report no transport");
}

#[test]
fn clear_via_set_none() {
    unsafe {
        set_custom_transport(Some(make_ops()));
        set_custom_transport(None);
    }
    assert!(peek_custom_transport().is_none());
}
