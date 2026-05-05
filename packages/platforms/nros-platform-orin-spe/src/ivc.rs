//! `PlatformIvc` impl — forwards to the `nvidia-ivc` driver crate's
//! zero-copy `nvidia_ivc_channel_*` C ABI (Phase 11.3.A).

use crate::OrinSpe;
use core::ffi::c_void;
use nros_platform_api::PlatformIvc;

impl PlatformIvc for OrinSpe {
    #[inline]
    fn channel_get(id: u32) -> *mut c_void {
        nvidia_ivc::nvidia_ivc_channel_get(id)
    }

    #[inline]
    fn frame_size(ch: *mut c_void) -> u32 {
        unsafe { nvidia_ivc::nvidia_ivc_channel_frame_size(ch) }
    }

    #[inline]
    fn rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
        unsafe { nvidia_ivc::nvidia_ivc_channel_rx_get(ch, len_out) }
    }

    #[inline]
    fn rx_release(ch: *mut c_void) {
        unsafe { nvidia_ivc::nvidia_ivc_channel_rx_release(ch) }
    }

    #[inline]
    fn tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
        unsafe { nvidia_ivc::nvidia_ivc_channel_tx_get(ch, cap_out) }
    }

    #[inline]
    fn tx_commit(ch: *mut c_void, len: usize) {
        unsafe { nvidia_ivc::nvidia_ivc_channel_tx_commit(ch, len) }
    }

    #[inline]
    fn tx_abandon(ch: *mut c_void) {
        unsafe { nvidia_ivc::nvidia_ivc_channel_tx_abandon(ch) }
    }

    #[inline]
    fn notify(ch: *mut c_void) {
        unsafe { nvidia_ivc::nvidia_ivc_channel_notify(ch) }
    }
}
