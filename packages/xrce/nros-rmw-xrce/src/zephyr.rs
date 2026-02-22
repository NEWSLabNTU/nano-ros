//! Zephyr BSD socket transport for XRCE-DDS.
//!
//! Declares extern "C" references to the transport callbacks defined in
//! `xrce_zephyr.c` (compiled by Zephyr CMake) and provides
//! [`init_zephyr_transport()`] to register them with the XRCE session.

use core::ffi::{c_char, c_int};

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

    fn xrce_zephyr_wait_network(timeout_ms: c_int) -> i32;
    fn xrce_zephyr_init(agent_addr: *const c_char, agent_port: c_int) -> i32;
}

/// Stack buffer for null-terminated agent address.
const ADDR_BUF_SIZE: usize = 48;

/// Initialize the Zephyr XRCE transport: wait for network, create UDP socket,
/// and register transport callbacks.
///
/// `locator` must be in "addr:port" format (e.g., "192.0.2.2:2018").
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
pub unsafe fn init_zephyr_transport(locator: &str) {
    // Wait for Zephyr network interface (5 second timeout)
    unsafe {
        let ret = xrce_zephyr_wait_network(5000);
        if ret != 0 {
            return;
        }
    }

    // Parse "addr:port" into separate address and port
    if let Some(colon_pos) = locator.rfind(':') {
        let addr_part = &locator[..colon_pos];
        let port_part = &locator[colon_pos + 1..];

        if let Ok(port) = port_part.parse::<u16>() {
            // Build null-terminated C string for the address
            let mut addr_buf = [0u8; ADDR_BUF_SIZE];
            let len = addr_part.len().min(ADDR_BUF_SIZE - 1);
            addr_buf[..len].copy_from_slice(&addr_part.as_bytes()[..len]);
            addr_buf[len] = 0;

            unsafe {
                xrce_zephyr_init(addr_buf.as_ptr() as *const c_char, port as c_int);
            }
        }
    }

    // Register transport callbacks
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
