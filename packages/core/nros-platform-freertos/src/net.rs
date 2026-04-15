//! lwIP BSD socket networking for FreeRTOS.
//!
//! Implements PlatformTcp, PlatformUdp, PlatformSocketHelpers, and
//! PlatformUdpMulticast on FreeRtosPlatform using lwIP's BSD socket API.
//! The lwIP library is linked by the board crate — these are just FFI
//! declarations resolved at link time.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::{c_int, c_void};

use crate::FreeRtosPlatform;

// ============================================================================
// lwIP BSD socket FFI declarations
// ============================================================================

// lwIP provides standard BSD socket API. These are resolved at link time
// from the lwIP library linked by the board crate.
// lwIP exports symbols with lwip_ prefix (the standard BSD names are C macros).
unsafe extern "C" {
    // Per-task lwIP socket initialization — must be called before any socket
    // operations in a FreeRTOS task. Without this, lwIP's internal per-task
    // socket state is uninitialized and socket calls silently fail.
    fn lwip_socket_thread_init();

    fn lwip_getaddrinfo(
        node: *const u8,
        service: *const u8,
        hints: *const AddrInfo,
        res: *mut *mut AddrInfo,
    ) -> c_int;
    fn lwip_freeaddrinfo(res: *mut AddrInfo);
    fn lwip_socket(domain: c_int, socktype: c_int, protocol: c_int) -> c_int;
    fn lwip_connect(fd: c_int, addr: *const SockAddr, addrlen: u32) -> c_int;
    fn lwip_bind(fd: c_int, addr: *const SockAddr, addrlen: u32) -> c_int;
    fn lwip_listen(fd: c_int, backlog: c_int) -> c_int;
    fn lwip_accept(fd: c_int, addr: *mut SockAddr, addrlen: *mut u32) -> c_int;
    fn lwip_recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> isize;
    fn lwip_send(fd: c_int, buf: *const c_void, len: usize, flags: c_int) -> isize;
    fn lwip_recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: usize,
        flags: c_int,
        addr: *mut SockAddr,
        addrlen: *mut u32,
    ) -> isize;
    fn lwip_sendto(
        fd: c_int,
        buf: *const c_void,
        len: usize,
        flags: c_int,
        addr: *const SockAddr,
        addrlen: u32,
    ) -> isize;
    fn lwip_setsockopt(
        fd: c_int,
        level: c_int,
        optname: c_int,
        optval: *const c_void,
        optlen: u32,
    ) -> c_int;
    fn lwip_close(fd: c_int) -> c_int;
    fn lwip_shutdown(fd: c_int, how: c_int) -> c_int;
    fn lwip_fcntl(fd: c_int, cmd: c_int, val: c_int) -> c_int;
}

// ============================================================================
// C struct layouts (must match lwIP / FreeRTOS headers)
// ============================================================================

/// BSD socket address (opaque, used as pointer only)
#[repr(C)]
struct SockAddr {
    _opaque: [u8; 16], // struct sockaddr is 16 bytes
}

/// addrinfo (matches lwIP's struct addrinfo layout)
#[repr(C)]
struct AddrInfo {
    ai_flags: c_int,
    ai_family: c_int,
    ai_socktype: c_int,
    ai_protocol: c_int,
    ai_addrlen: u32,
    ai_addr: *mut SockAddr,
    ai_canonname: *mut u8,
    ai_next: *mut AddrInfo,
}

/// Socket: `{ int _socket; }` (matches zenoh-pico FreeRTOS/lwIP type)
#[repr(C)]
struct Socket {
    _socket: c_int,
}

/// Endpoint: `{ struct addrinfo *_iptcp; }` (matches zenoh-pico FreeRTOS/lwIP type)
#[repr(C)]
struct Endpoint {
    _iptcp: *mut AddrInfo,
}

/// Timeval for socket timeouts
#[repr(C)]
struct Timeval {
    tv_sec: i32, // lwIP uses long = i32 on ARM32
    tv_usec: i32,
}

/// Linger option
#[repr(C)]
struct Linger {
    l_onoff: c_int,
    l_linger: c_int,
}

// Socket constants (match lwIP defines)
const PF_UNSPEC: c_int = 0;
const SOCK_STREAM: c_int = 1;
const SOCK_DGRAM: c_int = 2;
const IPPROTO_TCP: c_int = 6;
const IPPROTO_UDP: c_int = 17;
const SOL_SOCKET: c_int = 0xFFF;
const SO_RCVTIMEO: c_int = 0x1006;
const SO_KEEPALIVE: c_int = 0x0008;
const SO_LINGER: c_int = 0x0080;
const TCP_NODELAY: c_int = 0x01;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const O_NONBLOCK: c_int = 1;
const SHUT_RDWR: c_int = 2;

const Z_TRANSPORT_LEASE: u32 = 10000; // ms

// ============================================================================
// TCP
// ============================================================================

impl FreeRtosPlatform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        unsafe { lwip_socket_thread_init() };
        let ep = ep as *mut Endpoint;
        let hints = AddrInfo {
            ai_flags: 0,
            ai_family: PF_UNSPEC,
            ai_socktype: SOCK_STREAM,
            ai_protocol: IPPROTO_TCP,
            ai_addrlen: 0,
            ai_addr: core::ptr::null_mut(),
            ai_canonname: core::ptr::null_mut(),
            ai_next: core::ptr::null_mut(),
        };

        let ret = unsafe { lwip_getaddrinfo(address, port, &hints, &mut (*ep)._iptcp) };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn tcp_free_endpoint(ep: *mut c_void) {
        let ep = ep as *mut Endpoint;
        unsafe {
            if !(*ep)._iptcp.is_null() {
                lwip_freeaddrinfo((*ep)._iptcp);
            }
        }
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        unsafe { lwip_socket_thread_init() };

        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { lwip_socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._socket = fd };

        // SO_RCVTIMEO
        let tv = Timeval {
            tv_sec: (timeout_ms / 1000) as i32,
            tv_usec: ((timeout_ms % 1000) * 1000) as i32,
        };
        if unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<Timeval>() as u32,
            )
        } < 0
        {
            unsafe { lwip_close(fd) };
            return -1;
        }

        // SO_KEEPALIVE
        let one: c_int = 1;
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                4,
            );
        }

        // TCP_NODELAY
        unsafe {
            lwip_setsockopt(
                fd,
                IPPROTO_TCP,
                TCP_NODELAY,
                &one as *const _ as *const c_void,
                4,
            );
        }

        // SO_LINGER
        let ling = Linger {
            l_onoff: 1,
            l_linger: (Z_TRANSPORT_LEASE / 1000) as c_int,
        };
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_LINGER,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<Linger>() as u32,
            );
        }

        // Connect — iterate through addrinfo list
        let mut it = rep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { lwip_connect(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                return 0;
            }
            it = ai.ai_next;
        }

        unsafe {
            lwip_close(fd);
            (*sock)._socket = -1;
        }
        -1
    }

    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { lwip_socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }

        let one: c_int = 1;
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                0x0004, /* SO_REUSEADDR */
                &one as *const _ as *const c_void,
                4,
            );
        }

        if unsafe { lwip_bind(fd, ai.ai_addr, ai.ai_addrlen) } < 0 {
            unsafe { lwip_close(fd) };
            return -1;
        }

        if unsafe { lwip_listen(fd, 1) } < 0 {
            unsafe { lwip_close(fd) };
            return -1;
        }

        // Set timeout
        let tv = Timeval {
            tv_sec: (Z_TRANSPORT_LEASE / 1000) as i32,
            tv_usec: 0,
        };
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<Timeval>() as u32,
            );
        }

        unsafe { (*sock)._socket = fd };
        0
    }

    pub fn tcp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        let fd = unsafe { (*sock)._socket };
        if fd >= 0 {
            unsafe {
                lwip_shutdown(fd, SHUT_RDWR);
                lwip_close(fd);
                (*sock)._socket = -1;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._socket < 0 {
            return usize::MAX;
        }
        let n = unsafe { lwip_recv(sock._socket, buf as *mut c_void, len, 0) };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._socket < 0 {
            return usize::MAX;
        }

        let mut total: usize = 0;
        while total < len {
            let n = unsafe { lwip_recv(sock._socket, buf.add(total) as *mut c_void, len - total, 0) };
            if n <= 0 {
                return usize::MAX;
            }
            total += n as usize;
        }
        total
    }

    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._socket < 0 {
            return usize::MAX;
        }

        let mut total: usize = 0;
        while total < len {
            let n = unsafe {
                lwip_send(
                    sock._socket,
                    buf.add(total) as *const c_void,
                    len - total,
                    0,
                )
            };
            if n <= 0 {
                return usize::MAX;
            }
            total += n as usize;
        }
        total
    }
}

// ============================================================================
// UDP
// ============================================================================

impl FreeRtosPlatform {
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        unsafe { lwip_socket_thread_init() };
        let ep = ep as *mut Endpoint;
        let hints = AddrInfo {
            ai_flags: 0,
            ai_family: PF_UNSPEC,
            ai_socktype: SOCK_DGRAM,
            ai_protocol: IPPROTO_UDP,
            ai_addrlen: 0,
            ai_addr: core::ptr::null_mut(),
            ai_canonname: core::ptr::null_mut(),
            ai_next: core::ptr::null_mut(),
        };

        let ret = unsafe { lwip_getaddrinfo(address, port, &hints, &mut (*ep)._iptcp) };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn udp_free_endpoint(ep: *mut c_void) {
        Self::tcp_free_endpoint(ep)
    }

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        unsafe { lwip_socket_thread_init() };

        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { lwip_socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._socket = fd };

        let tv = Timeval {
            tv_sec: (timeout_ms / 1000) as i32,
            tv_usec: ((timeout_ms % 1000) * 1000) as i32,
        };
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<Timeval>() as u32,
            );
        }

        0
    }

    pub fn udp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        let fd = unsafe { (*sock)._socket };
        if fd >= 0 {
            unsafe {
                lwip_close(fd);
                (*sock)._socket = -1;
            }
        }
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._socket < 0 {
            return usize::MAX;
        }
        let n = unsafe {
            lwip_recvfrom(
                sock._socket,
                buf as *mut c_void,
                len,
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        // UDP is datagram-based — read_exact reads one datagram
        Self::udp_read(sock, buf, len)
    }

    pub fn udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        if sock._socket < 0 || rep._iptcp.is_null() {
            return usize::MAX;
        }
        let ai = unsafe { &*rep._iptcp };
        let n = unsafe {
            lwip_sendto(
                sock._socket,
                buf as *const c_void,
                len,
                0,
                ai.ai_addr,
                ai.ai_addrlen,
            )
        };
        if n <= 0 { usize::MAX } else { n as usize }
    }
}

// ============================================================================
// Socket helpers
// ============================================================================

impl FreeRtosPlatform {
    pub fn socket_set_non_blocking(sock: *const c_void) -> i8 {
        let sock = unsafe { &*(sock as *const Socket) };
        let flags = unsafe { lwip_fcntl(sock._socket, F_GETFL, 0) };
        if flags < 0 {
            return -1;
        }
        if unsafe { lwip_fcntl(sock._socket, F_SETFL, flags | O_NONBLOCK) } < 0 {
            return -1;
        }
        0
    }

    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        let sin = unsafe { &*(sock_in as *const Socket) };
        let sout = sock_out as *mut Socket;
        let mut addr: SockAddr = unsafe { core::mem::zeroed() };
        let mut addrlen: u32 = core::mem::size_of::<SockAddr>() as u32;

        let fd = unsafe { lwip_accept(sin._socket, &mut addr, &mut addrlen) };
        if fd < 0 {
            return -1;
        }

        // Set timeout + options on accepted socket
        let tv = Timeval {
            tv_sec: (Z_TRANSPORT_LEASE / 1000) as i32,
            tv_usec: 0,
        };
        unsafe {
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<Timeval>() as u32,
            );
            let one: c_int = 1;
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                4,
            );
            lwip_setsockopt(
                fd,
                IPPROTO_TCP,
                TCP_NODELAY,
                &one as *const _ as *const c_void,
                4,
            );
            let ling = Linger {
                l_onoff: 1,
                l_linger: (Z_TRANSPORT_LEASE / 1000) as c_int,
            };
            lwip_setsockopt(
                fd,
                SOL_SOCKET,
                SO_LINGER,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<Linger>() as u32,
            );
            (*sout)._socket = fd;
        }
        0
    }

    pub fn socket_close(sock: *mut c_void) {
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
        // FreeRTOS: yield to allow other tasks (including network) to run
        unsafe {
            vTaskDelay(1);
        }
        0
    }
}

// vTaskDelay FFI
unsafe extern "C" {
    fn vTaskDelay(ticks: u32);
}

// ============================================================================
// UDP multicast (stubs — not supported on FreeRTOS bare-metal)
// ============================================================================

impl FreeRtosPlatform {
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
