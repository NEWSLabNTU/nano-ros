//! Zephyr BSD socket transport for XRCE-DDS.
//!
//! Declares extern "C" references to the transport callbacks defined in
//! `xrce_zephyr.c` (compiled by Zephyr CMake) and provides
//! [`init_zephyr_transport()`] to register them with the XRCE session.

use core::ffi::c_int;

// The actual transport callbacks are implemented in xrce_zephyr.c,
// compiled by Zephyr's CMake build system (which has access to Zephyr headers).
// We declare them here as extern "C" so Rust can pass them as function pointers
// to uxr_set_custom_transport_callbacks() via init_transport().
unsafe extern "C" {
    fn xrce_zephyr_transport_open(transport: *mut xrce_sys::uxrCustomTransport) -> bool;
    fn xrce_zephyr_transport_close(transport: *mut xrce_sys::uxrCustomTransport) -> bool;
    fn xrce_zephyr_transport_write(
        transport: *mut xrce_sys::uxrCustomTransport,
        buffer: *const u8,
        length: usize,
        error_code: *mut u8,
    ) -> usize;
    fn xrce_zephyr_transport_read(
        transport: *mut xrce_sys::uxrCustomTransport,
        buffer: *mut u8,
        length: usize,
        timeout: c_int,
        error_code: *mut u8,
    ) -> usize;
}

/// Register the Zephyr BSD socket transport callbacks with the XRCE session.
///
/// Must be called after `xrce_zephyr_init()` (which creates the UDP socket)
/// and before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
/// The Zephyr UDP socket must already be connected (via `xrce_zephyr_init()`).
pub unsafe fn init_zephyr_transport() {
    unsafe {
        crate::init_transport(
            Some(xrce_zephyr_transport_open),
            Some(xrce_zephyr_transport_close),
            Some(xrce_zephyr_transport_write),
            Some(xrce_zephyr_transport_read),
            false, // UDP is packet-oriented, no framing needed
        );
    }
}
