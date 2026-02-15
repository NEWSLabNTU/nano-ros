//! POSIX UDP transport for XRCE-DDS native integration tests.
//!
//! Provides UDP custom transport callbacks using `std::net::UdpSocket`.

#![allow(static_mut_refs)]

use std::ffi::c_int;
use std::net::UdpSocket;
use std::time::Duration;

// ============================================================================
// Global Transport State
// ============================================================================

static mut AGENT_ADDR: [u8; 64] = [0u8; 64];
static mut AGENT_ADDR_LEN: usize = 0;
static mut UDP_SOCKET: Option<UdpSocket> = None;

/// Store the XRCE Agent address and initialize the transport callbacks.
///
/// Must be called before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
pub unsafe fn init_posix_udp_transport(agent_addr: &str) {
    unsafe {
        let len = agent_addr.len().min(63);
        AGENT_ADDR[..len].copy_from_slice(&agent_addr.as_bytes()[..len]);
        AGENT_ADDR[len] = 0;
        AGENT_ADDR_LEN = len;

        crate::init_transport(
            Some(transport_open),
            Some(transport_close),
            Some(transport_write),
            Some(transport_read),
        );
    }
}

unsafe extern "C" fn transport_open(
    _transport: *mut xrce_sys::uxrCustomTransport,
) -> bool {
    unsafe {
        let addr_str = core::str::from_utf8(&AGENT_ADDR[..AGENT_ADDR_LEN])
            .unwrap_or("127.0.0.1:2019");
        match UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => {
                if socket.connect(addr_str).is_ok() {
                    UDP_SOCKET = Some(socket);
                    true
                } else {
                    eprintln!("Failed to connect to XRCE Agent at {}", addr_str);
                    false
                }
            }
            Err(e) => {
                eprintln!("Failed to bind UDP socket: {}", e);
                false
            }
        }
    }
}

unsafe extern "C" fn transport_close(
    _transport: *mut xrce_sys::uxrCustomTransport,
) -> bool {
    unsafe {
        UDP_SOCKET = None;
        true
    }
}

unsafe extern "C" fn transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    unsafe {
        let data = core::slice::from_raw_parts(buffer, length);
        if let Some(ref socket) = UDP_SOCKET {
            match socket.send(data) {
                Ok(n) => n,
                Err(_) => {
                    *error_code = 1;
                    0
                }
            }
        } else {
            *error_code = 1;
            0
        }
    }
}

unsafe extern "C" fn transport_read(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if let Some(ref socket) = UDP_SOCKET {
            let timeout_duration = if timeout > 0 {
                Some(Duration::from_millis(timeout as u64))
            } else if timeout == 0 {
                Some(Duration::from_millis(1))
            } else {
                None // infinite timeout
            };
            let _ = socket.set_read_timeout(timeout_duration);

            let buf = core::slice::from_raw_parts_mut(buffer, length);
            match socket.recv(buf) {
                Ok(n) => n,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut
                    {
                        0 // timeout — return 0 bytes, not an error
                    } else {
                        *error_code = 1;
                        0
                    }
                }
            }
        } else {
            *error_code = 1;
            0
        }
    }
}
