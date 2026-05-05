//! Phase 100.4 + 11.3.A — IVC link-transport zero-copy forwarders.
//!
//! Nine `extern "C"` functions consumed by zenoh-pico's
//! `link/unicast/ivc.c`. They dispatch through `<P as PlatformIvc>`
//! into the active platform impl — `nros-platform-orin-spe::OrinSpe`
//! on the SPE / unix-mock host path.
//!
//! Independent of the rest of the shim: this module compiles whenever
//! `feature = "link-ivc"` is on, even if `feature = "active"` is off.
//! Use case is orin-spe (Phase 11.3.B), where zenoh-pico's
//! `src/system/freertos/system.c` provides clock/mutex/condvar/etc.
//! natively via FSP V10.4.3 FreeRTOS primitives, and the shim only
//! contributes the link-IVC C ABI.

use core::ffi::c_void;

use nros_platform::{ConcretePlatform, PlatformIvc};

type P = ConcretePlatform;

#[unsafe(no_mangle)]
pub extern "C" fn _z_open_ivc(channel_id: u32) -> *mut c_void {
    <P as PlatformIvc>::channel_get(channel_id)
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
    <P as PlatformIvc>::notify(ch)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_frame_size(ch: *mut c_void) -> u32 {
    <P as PlatformIvc>::frame_size(ch)
}

// Zero-copy RX path (Phase 11.3.A).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    <P as PlatformIvc>::rx_get(ch, len_out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_rx_release(ch: *mut c_void) {
    <P as PlatformIvc>::rx_release(ch)
}

// Zero-copy TX path (Phase 11.3.A).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    <P as PlatformIvc>::tx_get(ch, cap_out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_commit(ch: *mut c_void, len: usize) {
    <P as PlatformIvc>::tx_commit(ch, len)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_tx_abandon(ch: *mut c_void) {
    <P as PlatformIvc>::tx_abandon(ch)
}
