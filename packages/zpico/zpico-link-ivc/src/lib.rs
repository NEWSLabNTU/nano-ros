//! Phase 129.D — zenoh-pico IVC link-layer forwarders.
//!
//! Carved out of `zpico-platform-shim` so the parent crate can be
//! deleted. The forwarders dispatch directly into `nvidia-ivc`'s
//! C ABI — same shape that lived under
//! `zpico-platform-shim::ivc_helpers` previously.
//!
//! Linkage: the crate is pulled in by `zpico-sys`'s `link-ivc`
//! feature. As long as the application also links a platform
//! provider that satisfies `nvidia_ivc_channel_*`, the
//! `_z_open_ivc` / `_z_ivc_*` symbols resolve.

#![no_std]

use core::ffi::c_void;

#[unsafe(no_mangle)]
pub extern "C" fn _z_open_ivc(channel_id: u32) -> *mut c_void {
    nvidia_ivc::nvidia_ivc_channel_get(channel_id)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_close_ivc(_ch: *mut c_void) {
    // No-op: FSP channels outlive the session; the unix-mock
    // registry owns the fd. Symbol exists for ABI completeness —
    // the link layer calls it from `_z_f_link_close_ivc` and
    // expects it to return.
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_notify(ch: *mut c_void) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_notify(ch) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_frame_size(ch: *mut c_void) -> u32 {
    unsafe { nvidia_ivc::nvidia_ivc_channel_frame_size(ch) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    unsafe { nvidia_ivc::nvidia_ivc_channel_rx_get(ch, len_out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_release(ch: *mut c_void) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_rx_release(ch) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    unsafe { nvidia_ivc::nvidia_ivc_channel_tx_get(ch, cap_out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_commit(ch: *mut c_void, len: usize) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_tx_commit(ch, len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_abandon(ch: *mut c_void) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_tx_abandon(ch) }
}
