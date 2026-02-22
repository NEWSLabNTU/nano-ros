//! Zenoh-pico UDP unicast platform symbols implemented in Rust
//!
//! Provides the 8 extern "C" functions that zenoh-pico expects for UDP
//! unicast transport on bare-metal platforms. Each function matches the
//! zenoh-pico platform API signature from `system/link/udp.h`.

use crate::bridge::SmoltcpBridge;
use crate::bridge::SOCKET_TIMEOUT_MS;
use crate::util::{parse_ip_address, parse_port};

// Re-use the same C types as TCP (identical layout on bare-metal)
use crate::tcp::{ZSysNetEndpoint, ZSysNetSocket};

/// zenoh-pico result type (i8)
type ZResult = i8;
const Z_RES_OK: ZResult = 0;
const Z_ERR_GENERIC: ZResult = -1;

// ============================================================================
// Endpoint Functions
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_create_endpoint_udp(
    ep: *mut ZSysNetEndpoint,
    s_address: *const u8,
    s_port: *const u8,
) -> ZResult {
    if ep.is_null() || s_address.is_null() || s_port.is_null() {
        return Z_ERR_GENERIC;
    }

    let ip = match unsafe { parse_ip_address(s_address) } {
        Some(ip) => ip,
        None => return Z_ERR_GENERIC,
    };

    let port = match unsafe { parse_port(s_port) } {
        Some(p) => p,
        None => return Z_ERR_GENERIC,
    };

    unsafe {
        (*ep)._ip = ip;
        (*ep)._port = port;
    }

    Z_RES_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_free_endpoint_udp(_ep: *mut ZSysNetEndpoint) {
    // No dynamic allocation, nothing to free
}

// ============================================================================
// Socket Lifecycle Functions
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_open_udp_unicast(
    sock: *mut ZSysNetSocket,
    rep: ZSysNetEndpoint,
    _tout: u32,
) -> ZResult {
    if sock.is_null() {
        return Z_ERR_GENERIC;
    }

    unsafe {
        (*sock)._handle = -1;
        (*sock)._connected = false;
    }

    // Allocate a UDP socket from the bridge
    let handle = SmoltcpBridge::udp_socket_open();
    if handle < 0 {
        return Z_ERR_GENERIC;
    }

    // Set remote endpoint
    if SmoltcpBridge::udp_socket_set_remote(handle, &rep._ip, rep._port) < 0 {
        SmoltcpBridge::udp_socket_close(handle);
        return Z_ERR_GENERIC;
    }

    // Bind to ephemeral local port via smoltcp poll
    // The UDP socket will be bound during the next poll cycle when it
    // tries to send data. For now, mark it as ready.
    unsafe {
        (*sock)._handle = handle as i8;
        (*sock)._connected = true; // UDP is connectionless
    }

    Z_RES_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_listen_udp_unicast(
    _sock: *mut ZSysNetSocket,
    _rep: ZSysNetEndpoint,
    _tout: u32,
) -> ZResult {
    // Server-side listening not supported in client mode
    Z_ERR_GENERIC
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_close_udp_unicast(sock: *mut ZSysNetSocket) {
    if sock.is_null() {
        return;
    }
    unsafe {
        let handle = (*sock)._handle;
        if handle >= 0 {
            SmoltcpBridge::udp_socket_close(handle as i32);
            (*sock)._handle = -1;
            (*sock)._connected = false;
        }
    }
}

// ============================================================================
// Socket I/O Functions
// ============================================================================

/// Read up to `len` bytes. Returns bytes read, or `usize::MAX` on error/timeout.
#[unsafe(no_mangle)]
pub extern "C" fn _z_read_udp_unicast(sock: ZSysNetSocket, ptr: *mut u8, len: usize) -> usize {
    if sock._handle < 0 || ptr.is_null() || len == 0 {
        return usize::MAX;
    }

    let handle = sock._handle as i32;
    let start = SmoltcpBridge::clock_now_ms();

    loop {
        SmoltcpBridge::poll_network();

        if SmoltcpBridge::udp_socket_can_recv(handle) {
            let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
            let received = SmoltcpBridge::udp_socket_recv(handle, buf);
            if received > 0 {
                return received as usize;
            }
        }

        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
            return usize::MAX;
        }
    }
}

/// Read exactly `len` bytes. Returns bytes read, or `usize::MAX` on error/timeout.
#[unsafe(no_mangle)]
pub extern "C" fn _z_read_exact_udp_unicast(
    sock: ZSysNetSocket,
    ptr: *mut u8,
    len: usize,
) -> usize {
    if sock._handle < 0 || ptr.is_null() {
        return usize::MAX;
    }

    if len == 0 {
        return 0;
    }

    let handle = sock._handle as i32;
    let mut total_read: usize = 0;
    let mut start = SmoltcpBridge::clock_now_ms();

    while total_read < len {
        SmoltcpBridge::poll_network();

        if SmoltcpBridge::udp_socket_can_recv(handle) {
            let remaining = len - total_read;
            let buf = unsafe { core::slice::from_raw_parts_mut(ptr.add(total_read), remaining) };
            let received = SmoltcpBridge::udp_socket_recv(handle, buf);
            if received > 0 {
                total_read += received as usize;
                // Reset timeout on progress
                start = SmoltcpBridge::clock_now_ms();
            }
        }

        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
            return usize::MAX;
        }
    }

    total_read
}

/// Send `len` bytes to the remote endpoint. Returns bytes sent, or `usize::MAX` on error/timeout.
#[unsafe(no_mangle)]
pub extern "C" fn _z_send_udp_unicast(
    sock: ZSysNetSocket,
    ptr: *const u8,
    len: usize,
    rep: ZSysNetEndpoint,
) -> usize {
    if sock._handle < 0 || ptr.is_null() {
        return usize::MAX;
    }

    if len == 0 {
        return 0;
    }

    let handle = sock._handle as i32;
    let mut total_sent: usize = 0;
    let mut start = SmoltcpBridge::clock_now_ms();

    while total_sent < len {
        SmoltcpBridge::poll_network();

        if SmoltcpBridge::udp_socket_can_send(handle) {
            let remaining = len - total_sent;
            let data = unsafe { core::slice::from_raw_parts(ptr.add(total_sent), remaining) };
            let sent = SmoltcpBridge::udp_socket_send(handle, data, &rep._ip, rep._port);
            if sent > 0 {
                total_sent += sent as usize;
                // Reset timeout on progress
                start = SmoltcpBridge::clock_now_ms();
            }
        }

        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
            return usize::MAX;
        }
    }

    total_sent
}
