//! Zenoh-pico TCP platform symbols — thin wrappers over nros-smoltcp
//!
//! Each function matches the zenoh-pico platform API signature expected
//! by the C library, delegating to `nros_smoltcp::SmoltcpBridge`.

use nros_smoltcp::SmoltcpBridge;
use nros_smoltcp::util::{parse_ip_address, parse_port};
use nros_smoltcp::{CONNECT_TIMEOUT_MS, SOCKET_TIMEOUT_MS};

// ============================================================================
// C types matching bare-metal/platform.h
// ============================================================================

/// Socket handle passed between zenoh-pico and platform layer.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ZSysNetSocket {
    pub(crate) _handle: i8,
    pub(crate) _connected: bool,
    #[cfg(feature = "link-tls")]
    pub(crate) _tls_sock: *mut core::ffi::c_void,
}

/// Network endpoint (IPv4 address + port).
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ZSysNetEndpoint {
    pub(crate) _ip: [u8; 4],
    pub(crate) _port: u16,
}

/// zenoh-pico result type (i8)
type ZResult = i8;
const Z_RES_OK: ZResult = 0;
const Z_ERR_GENERIC: ZResult = -1;
const Z_ERR_TRANSPORT_TX_FAILED: ZResult = -1;

// ============================================================================
// Endpoint Functions
// ============================================================================

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_create_endpoint_tcp(
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

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_free_endpoint_tcp(_ep: *mut ZSysNetEndpoint) {
    // No dynamic allocation, nothing to free
}

// ============================================================================
// Socket Lifecycle Functions
// ============================================================================

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_open_tcp(
    sock: *mut ZSysNetSocket,
    rep: ZSysNetEndpoint,
    tout: u32,
) -> ZResult {
    if sock.is_null() {
        return Z_ERR_GENERIC;
    }

    unsafe {
        (*sock)._handle = -1;
        (*sock)._connected = false;
        #[cfg(feature = "link-tls")]
        {
            (*sock)._tls_sock = core::ptr::null_mut();
        }
    }

    let handle = SmoltcpBridge::tcp_open();
    if handle < 0 {
        return Z_ERR_GENERIC;
    }

    unsafe {
        (*sock)._handle = handle as i8;
    }

    if SmoltcpBridge::tcp_connect(handle, &rep._ip, rep._port) < 0 {
        SmoltcpBridge::tcp_close(handle);
        unsafe {
            (*sock)._handle = -1;
        }
        return Z_ERR_GENERIC;
    }

    // Wait for connection with timeout
    let timeout_ms = if tout > 0 {
        tout as u64
    } else {
        CONNECT_TIMEOUT_MS
    };
    let start = SmoltcpBridge::clock_now_ms();

    loop {
        SmoltcpBridge::poll_network();

        if SmoltcpBridge::tcp_is_connected(handle) {
            unsafe {
                (*sock)._connected = true;
            }
            return Z_RES_OK;
        }

        if SmoltcpBridge::clock_now_ms() - start > timeout_ms {
            SmoltcpBridge::tcp_close(handle);
            unsafe {
                (*sock)._handle = -1;
            }
            return Z_ERR_TRANSPORT_TX_FAILED;
        }
    }
}

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_listen_tcp(
    _sock: *mut ZSysNetSocket,
    _rep: ZSysNetEndpoint,
) -> ZResult {
    Z_ERR_GENERIC
}

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_close_tcp(sock: *mut ZSysNetSocket) {
    if sock.is_null() {
        return;
    }
    unsafe {
        let handle = (*sock)._handle;
        if handle >= 0 {
            SmoltcpBridge::tcp_close(handle as i32);
            (*sock)._handle = -1;
            (*sock)._connected = false;
        }
    }
}

// ============================================================================
// Socket I/O Functions
// ============================================================================

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_read_tcp(sock: ZSysNetSocket, ptr: *mut u8, len: usize) -> usize {
    if sock._handle < 0 || ptr.is_null() || len == 0 {
        return usize::MAX;
    }

    let handle = sock._handle as i32;

    SmoltcpBridge::poll_network();

    if SmoltcpBridge::tcp_can_recv(handle) {
        let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        let received = SmoltcpBridge::tcp_recv(handle, buf);
        if received > 0 {
            return received as usize;
        }
    }

    if !SmoltcpBridge::tcp_is_connected(handle) {
        return usize::MAX;
    }

    0
}

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_read_exact_tcp(sock: ZSysNetSocket, ptr: *mut u8, len: usize) -> usize {
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

        if SmoltcpBridge::tcp_can_recv(handle) {
            let remaining = len - total_read;
            let buf =
                unsafe { core::slice::from_raw_parts_mut(ptr.add(total_read), remaining) };
            let received = SmoltcpBridge::tcp_recv(handle, buf);
            if received > 0 {
                total_read += received as usize;
                start = SmoltcpBridge::clock_now_ms();
            }
        }

        if !SmoltcpBridge::tcp_is_connected(handle) {
            return usize::MAX;
        }

        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
            return usize::MAX;
        }
    }

    total_read
}

#[cfg_attr(not(feature = "no-export"), unsafe(no_mangle))]
pub extern "C" fn _z_send_tcp(sock: ZSysNetSocket, ptr: *const u8, len: usize) -> usize {
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

        if SmoltcpBridge::tcp_can_send(handle) {
            let remaining = len - total_sent;
            let data =
                unsafe { core::slice::from_raw_parts(ptr.add(total_sent), remaining) };
            let sent = SmoltcpBridge::tcp_send(handle, data);
            if sent > 0 {
                total_sent += sent as usize;
                start = SmoltcpBridge::clock_now_ms();
            }
        }

        if !SmoltcpBridge::tcp_is_connected(handle) {
            return usize::MAX;
        }

        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
            return usize::MAX;
        }
    }

    // Flush
    SmoltcpBridge::poll_network();

    total_sent
}
