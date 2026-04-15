//! smoltcp TCP/UDP networking via nros-smoltcp.
//!
//! Implements the PlatformTcp, PlatformUdp, and PlatformSocketHelpers
//! signatures on Mps2An385Platform by delegating to nros_smoltcp::SmoltcpBridge.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::c_void;

use crate::Mps2An385Platform;
use nros_smoltcp::SmoltcpBridge;
use nros_smoltcp::{CONNECT_TIMEOUT_MS, SOCKET_TIMEOUT_MS};

// ============================================================================
// C struct layouts (must match bare-metal/platform.h)
// ============================================================================

/// Socket: `{ int8_t _handle; bool _connected; [void* _tls_sock;] }`
#[repr(C)]
struct Socket {
    _handle: i8,
    _connected: bool,
}

/// Endpoint: `{ uint8_t _ip[4]; uint16_t _port; }`
#[repr(C)]
struct Endpoint {
    _ip: [u8; 4],
    _port: u16,
}

// ============================================================================
// IP / port parsing (from null-terminated C strings)
// ============================================================================

fn parse_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
    let ep = ep as *mut Endpoint;

    let ip = match unsafe { nros_smoltcp::util::parse_ip_address(address) } {
        Some(ip) => ip,
        None => return -1,
    };

    let p = match unsafe { nros_smoltcp::util::parse_port(port) } {
        Some(p) => p,
        None => return -1,
    };

    unsafe {
        (*ep)._ip = ip;
        (*ep)._port = p;
    }
    0
}

// ============================================================================
// TCP
// ============================================================================

impl Mps2An385Platform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        if ep.is_null() || address.is_null() || port.is_null() {
            return -1;
        }
        parse_endpoint(ep, address, port)
    }

    pub fn tcp_free_endpoint(_ep: *mut c_void) {
        // No dynamic allocation
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        if sock.is_null() || endpoint.is_null() {
            return -1;
        }

        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        unsafe {
            (*sock)._handle = -1;
            (*sock)._connected = false;
        }

        let handle = SmoltcpBridge::tcp_open();
        if handle < 0 {
            return -1;
        }

        unsafe {
            (*sock)._handle = handle as i8;
        }

        if SmoltcpBridge::tcp_connect(handle, &rep._ip, rep._port) < 0 {
            SmoltcpBridge::tcp_close(handle);
            unsafe {
                (*sock)._handle = -1;
            }
            return -1;
        }

        // Wait for connection with timeout
        let timeout = if timeout_ms > 0 {
            timeout_ms as u64
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
                return 0;
            }

            if SmoltcpBridge::clock_now_ms() - start > timeout {
                SmoltcpBridge::tcp_close(handle);
                unsafe {
                    (*sock)._handle = -1;
                }
                return -1;
            }
        }
    }

    pub fn tcp_listen(_sock: *mut c_void, _endpoint: *const c_void) -> i8 {
        -1 // Server-side not supported
    }

    pub fn tcp_close(sock: *mut c_void) {
        if sock.is_null() {
            return;
        }
        let sock = sock as *mut Socket;
        unsafe {
            let handle = (*sock)._handle;
            if handle >= 0 {
                SmoltcpBridge::tcp_close(handle as i32);
                (*sock)._handle = -1;
                (*sock)._connected = false;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() || len == 0 {
            return usize::MAX;
        }

        let handle = sock._handle as i32;

        SmoltcpBridge::poll_network();

        if SmoltcpBridge::tcp_can_recv(handle) {
            let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
            let received = SmoltcpBridge::tcp_recv(handle, slice);
            if received > 0 {
                return received as usize;
            }
        }

        if !SmoltcpBridge::tcp_is_connected(handle) {
            return usize::MAX;
        }

        0
    }

    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() {
            return usize::MAX;
        }
        if len == 0 {
            return 0;
        }

        let handle = sock._handle as i32;
        let mut total: usize = 0;
        let mut start = SmoltcpBridge::clock_now_ms();

        while total < len {
            SmoltcpBridge::poll_network();

            if SmoltcpBridge::tcp_can_recv(handle) {
                let remaining = len - total;
                let slice =
                    unsafe { core::slice::from_raw_parts_mut(buf.add(total), remaining) };
                let received = SmoltcpBridge::tcp_recv(handle, slice);
                if received > 0 {
                    total += received as usize;
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

        total
    }

    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() {
            return usize::MAX;
        }
        if len == 0 {
            return 0;
        }

        let handle = sock._handle as i32;
        let mut total: usize = 0;
        let mut start = SmoltcpBridge::clock_now_ms();

        while total < len {
            SmoltcpBridge::poll_network();

            if SmoltcpBridge::tcp_can_send(handle) {
                let remaining = len - total;
                let data =
                    unsafe { core::slice::from_raw_parts(buf.add(total), remaining) };
                let sent = SmoltcpBridge::tcp_send(handle, data);
                if sent > 0 {
                    total += sent as usize;
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

        total
    }
}

// ============================================================================
// UDP
// ============================================================================

impl Mps2An385Platform {
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        if ep.is_null() || address.is_null() || port.is_null() {
            return -1;
        }
        parse_endpoint(ep, address, port)
    }

    pub fn udp_free_endpoint(_ep: *mut c_void) {
        // No dynamic allocation
    }

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, _timeout_ms: u32) -> i8 {
        if sock.is_null() || endpoint.is_null() {
            return -1;
        }

        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        unsafe {
            (*sock)._handle = -1;
            (*sock)._connected = false;
        }

        let handle = SmoltcpBridge::udp_open();
        if handle < 0 {
            return -1;
        }

        if SmoltcpBridge::udp_set_remote(handle, &rep._ip, rep._port) < 0 {
            SmoltcpBridge::udp_close(handle);
            return -1;
        }

        unsafe {
            (*sock)._handle = handle as i8;
            (*sock)._connected = true; // UDP is connectionless
        }

        0
    }

    pub fn udp_close(sock: *mut c_void) {
        if sock.is_null() {
            return;
        }
        let sock = sock as *mut Socket;
        unsafe {
            let handle = (*sock)._handle;
            if handle >= 0 {
                SmoltcpBridge::udp_close(handle as i32);
                (*sock)._handle = -1;
                (*sock)._connected = false;
            }
        }
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() || len == 0 {
            return usize::MAX;
        }

        let handle = sock._handle as i32;
        let start = SmoltcpBridge::clock_now_ms();

        loop {
            SmoltcpBridge::poll_network();

            if SmoltcpBridge::udp_can_recv(handle) {
                let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
                let received = SmoltcpBridge::udp_recv(handle, slice);
                if received > 0 {
                    return received as usize;
                }
            }

            if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                return usize::MAX;
            }
        }
    }

    pub fn udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() {
            return usize::MAX;
        }
        if len == 0 {
            return 0;
        }

        let handle = sock._handle as i32;
        let mut total: usize = 0;
        let mut start = SmoltcpBridge::clock_now_ms();

        while total < len {
            SmoltcpBridge::poll_network();

            if SmoltcpBridge::udp_can_recv(handle) {
                let remaining = len - total;
                let slice =
                    unsafe { core::slice::from_raw_parts_mut(buf.add(total), remaining) };
                let received = SmoltcpBridge::udp_recv(handle, slice);
                if received > 0 {
                    total += received as usize;
                    start = SmoltcpBridge::clock_now_ms();
                }
            }

            if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                return usize::MAX;
            }
        }

        total
    }

    pub fn udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._handle < 0 || buf.is_null() {
            return usize::MAX;
        }
        if len == 0 {
            return 0;
        }

        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let handle = sock._handle as i32;
        let mut total: usize = 0;
        let mut start = SmoltcpBridge::clock_now_ms();

        while total < len {
            SmoltcpBridge::poll_network();

            if SmoltcpBridge::udp_can_send(handle) {
                let remaining = len - total;
                let data =
                    unsafe { core::slice::from_raw_parts(buf.add(total), remaining) };
                let sent = SmoltcpBridge::udp_send(handle, data, &rep._ip, rep._port);
                if sent > 0 {
                    total += sent as usize;
                    start = SmoltcpBridge::clock_now_ms();
                }
            }

            if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                return usize::MAX;
            }
        }

        total
    }
}

// ============================================================================
// Socket helpers
// ============================================================================

impl Mps2An385Platform {
    pub fn socket_set_non_blocking(_sock: *const c_void) -> i8 {
        0 // smoltcp is always non-blocking
    }

    pub fn socket_accept(_sock_in: *const c_void, _sock_out: *mut c_void) -> i8 {
        -1 // Not supported in client mode
    }

    pub fn socket_close(sock: *mut c_void) {
        // Delegate to tcp_close (same struct layout)
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
        SmoltcpBridge::poll_network();
        0
    }
}

// ============================================================================
// UDP multicast (not supported on bare-metal — stubs return errors)
// ============================================================================

impl Mps2An385Platform {
    pub fn mcast_open(
        _sock: *mut c_void,
        _endpoint: *const c_void,
        _lep: *mut c_void,
        _timeout_ms: u32,
        _iface: *const u8,
    ) -> i8 {
        -1
    }

    pub fn mcast_listen(
        _sock: *mut c_void,
        _endpoint: *const c_void,
        _timeout_ms: u32,
        _iface: *const u8,
        _join: *const u8,
    ) -> i8 {
        -1
    }

    pub fn mcast_close(
        _sockrecv: *mut c_void,
        _socksend: *mut c_void,
        _rep: *const c_void,
        _lep: *const c_void,
    ) {
    }

    pub fn mcast_read(
        _sock: *const c_void,
        _buf: *mut u8,
        _len: usize,
        _lep: *const c_void,
        _addr: *mut c_void,
    ) -> usize {
        usize::MAX
    }

    pub fn mcast_read_exact(
        _sock: *const c_void,
        _buf: *mut u8,
        _len: usize,
        _lep: *const c_void,
        _addr: *mut c_void,
    ) -> usize {
        usize::MAX
    }

    pub fn mcast_send(
        _sock: *const c_void,
        _buf: *const u8,
        _len: usize,
        _endpoint: *const c_void,
    ) -> usize {
        usize::MAX
    }
}
