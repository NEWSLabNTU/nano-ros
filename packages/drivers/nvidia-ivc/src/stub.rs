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
pub(crate) unsafe fn read(_ch: *mut c_void, _buf: *mut u8, _len: usize) -> usize {
    usize::MAX
}

#[inline]
pub(crate) unsafe fn write(_ch: *mut c_void, _buf: *const u8, _len: usize) -> usize {
    usize::MAX
}

#[inline]
pub(crate) unsafe fn notify(_ch: *mut c_void) {}

#[inline]
pub(crate) unsafe fn frame_size(_ch: *mut c_void) -> u32 {
    0
}
