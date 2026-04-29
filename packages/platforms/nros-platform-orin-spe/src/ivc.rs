//! `PlatformIvc` impl — forwards to the `nvidia-ivc` driver crate.
//!
//! The driver crate already exposes both backends (`fsp` for hardware,
//! `unix-mock` for the FreeRTOS POSIX-simulator dev path) behind feature
//! flags; this file just lifts that into the `PlatformIvc` trait shape
//! that `zpico-platform-shim::ivc_helpers` and zenoh-pico's
//! `Z_FEATURE_LINK_IVC` C code consume.

use crate::OrinSpe;
use core::ffi::c_void;
use nros_platform_api::PlatformIvc;

impl PlatformIvc for OrinSpe {
    #[inline]
    fn channel_get(id: u32) -> *mut c_void {
        nvidia_ivc::nvidia_ivc_channel_get(id)
    }

    #[inline]
    fn read(ch: *mut c_void, buf: *mut u8, len: usize) -> usize {
        unsafe { nvidia_ivc::nvidia_ivc_channel_read(ch, buf, len) }
    }

    #[inline]
    fn write(ch: *mut c_void, buf: *const u8, len: usize) -> usize {
        unsafe { nvidia_ivc::nvidia_ivc_channel_write(ch, buf, len) }
    }

    #[inline]
    fn notify(ch: *mut c_void) {
        unsafe { nvidia_ivc::nvidia_ivc_channel_notify(ch) }
    }

    #[inline]
    fn frame_size(ch: *mut c_void) -> u32 {
        unsafe { nvidia_ivc::nvidia_ivc_channel_frame_size(ch) }
    }
}
