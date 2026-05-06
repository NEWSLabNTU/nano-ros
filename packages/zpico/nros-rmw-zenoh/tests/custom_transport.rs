//! Phase 115.B — zenoh-pico custom-link smoke test.
//!
//! Exercises the full `custom://` round-trip:
//!
//! 1. Register a stub `NrosTransportOps` via
//!    `nros_rmw::set_custom_transport(...)`.
//! 2. Open a `ZenohSession` against locator `custom:///` in client
//!    mode.
//! 3. zenoh-pico's link factory drains the slot, calls our stub's
//!    `open()` callback, and starts driving `read()` from the
//!    session reader thread.
//! 4. Confirm the stub's `open()` counter bumped — proof that the
//!    `Z_FEATURE_LINK_CUSTOM` codepath is wired end-to-end.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-rmw-zenoh --features "platform-posix,link-tcp,link-custom" \
//!     --test custom_transport
//! ```
//!
//! `link-tcp` is required transitively (zenoh-pico refuses a
//! no-link build); the `custom://` locator selects which link
//! actually opens.

#![cfg(all(feature = "platform-posix", feature = "link-custom"))]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};
use std::{thread, time::Duration};

use nros_rmw::{
    NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, SessionMode, Transport, TransportConfig,
    set_custom_transport,
};
use nros_rmw_zenoh::ZenohTransport;

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
unsafe extern "C" fn stub_write(_ud: *mut c_void, _buf: *const u8, _len: usize) -> i32 {
    WRITE_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn stub_read(_ud: *mut c_void, _buf: *mut u8, _len: usize, _to: u32) -> i32 {
    READ_CALLS.fetch_add(1, Ordering::Relaxed);
    0 // zero bytes, no error
}

fn make_ops() -> NrosTransportOps {
    NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: 0xDEAD_BEEF_usize as *mut c_void,
        open: stub_open,
        close: stub_close,
        write: stub_write,
        read: stub_read,
    }
}

#[test]
fn custom_locator_routes_through_user_vtable() {
    OPEN_CALLS.store(0, Ordering::Relaxed);
    CLOSE_CALLS.store(0, Ordering::Relaxed);
    WRITE_CALLS.store(0, Ordering::Relaxed);
    READ_CALLS.store(0, Ordering::Relaxed);

    // Step 1: register the vtable.
    unsafe { set_custom_transport(Some(make_ops())).expect("set v1 ok") };

    // Step 2: open zenoh session against `custom/anywhere`. zenoh-pico's
    // link factory dispatches by scheme — picks our custom link,
    // which drains the slot and calls stub_open(). The address
    // segment is opaque to v1 of the link (zero configurable keys);
    // we just need a non-empty value so the locator parser accepts.
    let config = TransportConfig {
        locator: Some("custom/anywhere"),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
    };

    // ZenohTransport::open returns ConnectionFailed because the
    // stub's read() yields 0 bytes — zenoh-pico can't complete the
    // INIT handshake against a black-hole transport. That's
    // expected for v1; the link layer still drove open() / write()
    // / read() / close() against our vtable.
    let _ = ZenohTransport::open(&config);

    // Give the session a moment to tear down cleanly so close()
    // counter has a chance to bump too.
    thread::sleep(Duration::from_millis(20));

    assert_eq!(
        OPEN_CALLS.load(Ordering::Relaxed),
        1,
        "open() must fire exactly once during session bring-up"
    );
    assert!(
        WRITE_CALLS.load(Ordering::Relaxed) >= 1,
        "write() must fire at least once (INIT message)"
    );
    assert!(
        READ_CALLS.load(Ordering::Relaxed) >= 1,
        "read() must fire at least once (INIT-ACK attempt)"
    );
    assert_eq!(
        CLOSE_CALLS.load(Ordering::Relaxed),
        1,
        "close() must fire on session-teardown"
    );
}
