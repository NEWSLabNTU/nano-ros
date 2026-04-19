//! Platform-agnostic UDP transport for XRCE-DDS via nros-platform.
//!
//! Replaces per-platform transport modules (posix_udp.rs, zephyr.rs) with
//! a single implementation that delegates to `ConcretePlatform::udp_*()`.
//! Covers POSIX, Zephyr, FreeRTOS, NuttX, and ThreadX.

#![allow(static_mut_refs)]

use core::ffi::{c_int, c_void};

use nros_platform::ConcretePlatform;

// ============================================================================
// Global Transport State
// ============================================================================

/// Conservatively-sized buffers for opaque socket/endpoint handles.
/// Largest known layout: POSIX Socket = 16 bytes, Endpoint = 8 bytes.
/// 32 bytes provides ample headroom for all platforms.
const HANDLE_SIZE: usize = 32;

static mut SOCK: [u8; HANDLE_SIZE] = [0u8; HANDLE_SIZE];
static mut ENDPOINT: [u8; HANDLE_SIZE] = [0u8; HANDLE_SIZE];

/// Agent address (null-terminated C string for `create_endpoint`).
const ADDR_BUF_SIZE: usize = 48;
static mut AGENT_ADDR: [u8; ADDR_BUF_SIZE] = [0u8; ADDR_BUF_SIZE];
/// Agent port (null-terminated C string for `create_endpoint`).
const PORT_BUF_SIZE: usize = 8;
static mut AGENT_PORT: [u8; PORT_BUF_SIZE] = [0u8; PORT_BUF_SIZE];

// ============================================================================
// Public API
// ============================================================================

/// Parse `locator` ("addr:port") and register XRCE transport callbacks.
///
/// Must be called before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
pub unsafe fn init_platform_udp_transport(locator: &str) {
    // Parse "addr:port"
    if let Some(colon_pos) = locator.rfind(':') {
        let addr_part = &locator[..colon_pos];
        let port_part = &locator[colon_pos + 1..];

        // Store address as null-terminated C string
        unsafe {
            let addr_len = addr_part.len().min(ADDR_BUF_SIZE - 1);
            AGENT_ADDR[..addr_len].copy_from_slice(&addr_part.as_bytes()[..addr_len]);
            AGENT_ADDR[addr_len] = 0;

            let port_len = port_part.len().min(PORT_BUF_SIZE - 1);
            AGENT_PORT[..port_len].copy_from_slice(&port_part.as_bytes()[..port_len]);
            AGENT_PORT[port_len] = 0;
        }
    }

    unsafe {
        crate::init_transport(
            Some(transport_open),
            Some(transport_close),
            Some(transport_write),
            Some(transport_read),
            false, // UDP is packet-oriented, no framing needed
        );
    }
}

// ============================================================================
// XRCE Transport Callbacks
// ============================================================================

unsafe extern "C" fn transport_open(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        // Create endpoint from stored address/port
        let ret = ConcretePlatform::udp_create_endpoint(
            ENDPOINT.as_mut_ptr() as *mut c_void,
            AGENT_ADDR.as_ptr(),
            AGENT_PORT.as_ptr(),
        );
        if ret < 0 {
            return false;
        }

        // Open UDP socket with a default timeout (will be overridden per-read)
        let ret = ConcretePlatform::udp_open(
            SOCK.as_mut_ptr() as *mut c_void,
            ENDPOINT.as_ptr() as *const c_void,
            1000, // 1s default timeout, overridden by set_recv_timeout on each read
        );
        ret >= 0
    }
}

unsafe extern "C" fn transport_close(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        ConcretePlatform::udp_close(SOCK.as_mut_ptr() as *mut c_void);
        ConcretePlatform::udp_free_endpoint(ENDPOINT.as_mut_ptr() as *mut c_void);
    }
    true
}

unsafe extern "C" fn transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    unsafe {
        let n = ConcretePlatform::udp_send(
            SOCK.as_ptr() as *const c_void,
            buffer,
            length,
            ENDPOINT.as_ptr() as *const c_void,
        );
        if n == usize::MAX {
            *error_code = 1;
            0
        } else {
            n
        }
    }
}

unsafe extern "C" fn transport_read(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    _error_code: *mut u8,
) -> usize {
    unsafe {
        // Set per-read timeout: >0 = use as ms, 0 = 1ms minimum, <0 = block forever
        let timeout_ms = if timeout > 0 {
            timeout as u32
        } else if timeout == 0 {
            1
        } else {
            0 // 0 means block indefinitely for setsockopt
        };
        ConcretePlatform::udp_set_recv_timeout(SOCK.as_ptr() as *const c_void, timeout_ms);

        let n = ConcretePlatform::udp_read(
            SOCK.as_ptr() as *const c_void,
            buffer,
            length,
        );
        if n == usize::MAX {
            // Timeout or error — not an error for XRCE (timeout returns 0)
            0
        } else {
            n
        }
    }
}
