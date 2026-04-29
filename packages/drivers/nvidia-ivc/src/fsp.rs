//! `fsp` backend — links NVIDIA's `tegra_aon_fsp.a` and forwards every
//! call to the matching `tegra_ivc_channel_*` symbol.
//!
//! NVIDIA's FSP exposes the SPE-side IVC API as plain C with a fixed
//! signature shape. We mirror it 1:1; conversion to the safe Rust
//! `Channel` API happens in `lib.rs`.
//!
//! No tests live here — exercising this backend requires a real Orin
//! SPE binary and the SDK Manager install. Phase 100.6 wires it into
//! `nros-board-orin-spe` and validates with hardware loopback.

use core::ffi::c_void;

unsafe extern "C" {
    fn tegra_ivc_channel_get(id: u32) -> *mut c_void;
    fn tegra_ivc_channel_read(ch: *mut c_void, buf: *mut u8, len: usize) -> isize;
    fn tegra_ivc_channel_write(ch: *mut c_void, buf: *const u8, len: usize) -> isize;
    fn tegra_ivc_channel_notify(ch: *mut c_void);
    fn tegra_ivc_channel_frame_size(ch: *mut c_void) -> u32;
}

#[inline]
pub(crate) fn channel_get(id: u32) -> *mut c_void {
    unsafe { tegra_ivc_channel_get(id) }
}

#[inline]
pub(crate) unsafe fn read(ch: *mut c_void, buf: *mut u8, len: usize) -> usize {
    let n = unsafe { tegra_ivc_channel_read(ch, buf, len) };
    if n < 0 { usize::MAX } else { n as usize }
}

#[inline]
pub(crate) unsafe fn write(ch: *mut c_void, buf: *const u8, len: usize) -> usize {
    let n = unsafe { tegra_ivc_channel_write(ch, buf, len) };
    if n < 0 { usize::MAX } else { n as usize }
}

#[inline]
pub(crate) unsafe fn notify(ch: *mut c_void) {
    unsafe { tegra_ivc_channel_notify(ch) }
}

#[inline]
pub(crate) unsafe fn frame_size(ch: *mut c_void) -> u32 {
    unsafe { tegra_ivc_channel_frame_size(ch) }
}
