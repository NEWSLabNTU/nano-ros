//! A custom link's receive path reaches the user transport's `read`.
//!
//! `custom_transport.rs` proves the custom link's callbacks are reached during
//! a session's bring-up. Bring-up reads through the link's `read_exact` entry,
//! though, and a session's reader reads through a different one,
//! `_z_link_socket_recv_zbuf`. When that function stopped routing a custom
//! link to the link's own read, a custom-link subscriber received nothing
//! while every bring-up check still passed (issue 2747).
//!
//! This asks that function directly, through the shim's
//! `zpico_custom_link_recv_probe`: open a link on `custom/probe`, receive once,
//! and compare the bytes with what the transport's `read` handed over. It needs
//! no session, no peer and no network.
//!
//! Runs in `just check required-features-tests` (`.config/feature-lanes.toml`).

#![cfg(all(feature = "platform-posix", feature = "link-custom"))]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

use nros_rmw::{NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, set_custom_transport};
// The probe lives in zpico-sys's archive, which this binary links only
// through the backend crate, so name it.
use nros_rmw_zenoh::ZenohTransport;

/// What the transport hands over on its first read.
const SENT: &[u8] = b"\x05\x00hello";

static READS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" {
    /// The shim's test seam (`zpico.c`): open `custom/probe`, receive once
    /// through `_z_link_socket_recv_zbuf`, close. The bytes received, or
    /// `usize::MAX`.
    fn zpico_custom_link_recv_probe(out: *mut u8, cap: usize) -> usize;
}

unsafe extern "C" fn open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    0
}
unsafe extern "C" fn close(_ud: *mut c_void) {}
unsafe extern "C" fn write(_ud: *mut c_void, _buf: *const u8, _len: usize) -> i32 {
    0
}
unsafe extern "C" fn read(_ud: *mut c_void, buf: *mut u8, len: usize, _timeout_ms: u32) -> i32 {
    READS.fetch_add(1, Ordering::Relaxed);
    let n = SENT.len().min(len);
    unsafe { core::ptr::copy_nonoverlapping(SENT.as_ptr(), buf, n) };
    i32::try_from(n).unwrap()
}

#[test]
fn a_custom_link_receives_through_the_transports_read() {
    let ops = NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: core::ptr::null_mut(),
        open,
        close,
        write,
        read,
    };
    unsafe { set_custom_transport(Some(ops)).expect("register the custom transport") };

    let _ = core::mem::size_of::<ZenohTransport>();
    let mut out = [0u8; 64];
    let got = unsafe { zpico_custom_link_recv_probe(out.as_mut_ptr(), out.len()) };

    assert_ne!(
        got,
        usize::MAX,
        "the custom link did not open, or its read failed"
    );
    assert!(
        READS.load(Ordering::Relaxed) >= 1,
        "the receive path never called the transport's read: it went to the \
         link's socket-shaped read, which a custom link cannot serve"
    );
    assert_eq!(
        &out[..got],
        SENT,
        "the bytes received are not the bytes read"
    );
}
