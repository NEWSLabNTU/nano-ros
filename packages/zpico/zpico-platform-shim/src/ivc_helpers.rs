//! Phase 100.4 + 11.3.A — IVC link-transport zero-copy forwarders.
//!
//! Nine `extern "C"` functions consumed by zenoh-pico's
//! `link/unicast/ivc.c`. They dispatch directly into the `nvidia-ivc`
//! driver crate's C ABI (`nvidia_ivc_channel_*`).
//!
//! Phase 121.10 — was previously routed through
//! `<ConcretePlatform as PlatformIvc>` against `nros-platform-orin-spe`.
//! With orin-spe demoted from a platform to a board over FreeRTOS,
//! IVC lives in the board layer, not the platform layer. The shim
//! calls the driver crate directly; no trait dispatch needed.
//!
//! Independent of the rest of the shim: this module compiles whenever
//! `feature = "link-ivc"` is on, even if `feature = "active"` is off.

use core::ffi::c_void;

#[unsafe(no_mangle)]
pub extern "C" fn _z_open_ivc(channel_id: u32) -> *mut c_void {
    nvidia_ivc::nvidia_ivc_channel_get(channel_id)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_close_ivc(_ch: *mut c_void) {
    // No-op on hardware (FSP channels outlive the session) and on
    // the unix-mock (the registry owns the fd). Symbol exists for
    // ABI completeness — the link layer calls it from
    // `_z_f_link_close_ivc` and expects it to return.
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_notify(ch: *mut c_void) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_notify(ch) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_frame_size(ch: *mut c_void) -> u32 {
    unsafe { nvidia_ivc::nvidia_ivc_channel_frame_size(ch) }
}

// Zero-copy RX path (Phase 11.3.A).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    unsafe { nvidia_ivc::nvidia_ivc_channel_rx_get(ch, len_out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_release(ch: *mut c_void) {
    unsafe { nvidia_ivc::nvidia_ivc_channel_rx_release(ch) }
}

// Zero-copy TX path (Phase 11.3.A).
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
