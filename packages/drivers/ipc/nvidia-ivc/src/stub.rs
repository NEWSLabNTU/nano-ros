//! No-feature stub: every call fails closed.
//!
//! Lets `cargo check -p nvidia-ivc` work without a backend selected
//! (useful for tooling like `cargo doc` and rust-analyzer scans). The
//! real consumers always select either `fsp` or `unix-mock`.

use core::ffi::c_void;

#[inline]
pub(crate) fn channel_get(_id: u32) -> *mut c_void {
    core::ptr::null_mut()
}

#[inline]
pub(crate) unsafe fn frame_size(_ch: *mut c_void) -> u32 {
    0
}

#[inline]
pub(crate) unsafe fn rx_get(_ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    if !len_out.is_null() {
        unsafe { *len_out = 0 };
    }
    core::ptr::null()
}

#[inline]
pub(crate) unsafe fn rx_release(_ch: *mut c_void) {}

#[inline]
pub(crate) unsafe fn tx_get(_ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    if !cap_out.is_null() {
        unsafe { *cap_out = 0 };
    }
    core::ptr::null_mut()
}

#[inline]
pub(crate) unsafe fn tx_commit(_ch: *mut c_void, _len: usize) {}

#[inline]
pub(crate) unsafe fn tx_abandon(_ch: *mut c_void) {}

#[inline]
pub(crate) unsafe fn notify(_ch: *mut c_void) {}
