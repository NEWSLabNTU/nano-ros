//! NetX Duo BSD socket networking for ThreadX.
//!
//! Implements PlatformTcp, PlatformUdp, PlatformSocketHelpers on
//! ThreadxPlatform using NetX Duo's BSD socket API (`nx_bsd_*`).
//! Manual FFI — the types are simple enough that bindgen isn't needed.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::{c_int, c_void};

use crate::ThreadxPlatform;

// ============================================================================
// NetX Duo BSD socket FFI
// ============================================================================

// NetX Duo BSD uses nx_bsd_* prefix to avoid conflicts with system headers.
#[allow(dead_code)]
unsafe extern "C" {
    fn nx_bsd_socket(domain: c_int, socktype: c_int, protocol: c_int) -> c_int;
    fn nx_bsd_connect(fd: c_int, addr: *const SockAddrIn, addrlen: c_int) -> c_int;
    fn nx_bsd_bind(fd: c_int, addr: *const SockAddrIn, addrlen: c_int) -> c_int;
    fn nx_bsd_listen(fd: c_int, backlog: c_int) -> c_int;
    fn nx_bsd_accept(fd: c_int, addr: *mut SockAddrIn, addrlen: *mut c_int) -> c_int;
    fn nx_bsd_recv(fd: c_int, buf: *mut u8, len: c_int, flags: c_int) -> c_int;
    fn nx_bsd_send(fd: c_int, buf: *const u8, len: c_int, flags: c_int) -> c_int;
    fn nx_bsd_recvfrom(
        fd: c_int,
        buf: *mut u8,
        len: c_int,
        flags: c_int,
        addr: *mut SockAddrIn,
        addrlen: *mut c_int,
    ) -> c_int;
    fn nx_bsd_sendto(
        fd: c_int,
        buf: *const u8,
        len: c_int,
        flags: c_int,
        addr: *const SockAddrIn,
        addrlen: c_int,
    ) -> c_int;
    fn nx_bsd_setsockopt(
        fd: c_int,
        level: c_int,
        optname: c_int,
        optval: *const c_void,
        optlen: c_int,
    ) -> c_int;
    fn nx_bsd_soc_close(fd: c_int) -> c_int;
    fn tx_thread_sleep(ticks: u32);
}

// Byte order conversion (NetX expects network byte order in sockaddr)
unsafe extern "C" {
    fn htonl(hostlong: u32) -> u32;
    fn htons(hostshort: u16) -> u16;
}

// ============================================================================
// C struct layouts
// ============================================================================

/// Socket: `{ int _fd; }` (matches _z_sys_net_socket_t)
#[repr(C)]
struct Socket {
    _fd: c_int,
}

/// Endpoint: `{ uint32_t _addr; uint16_t _port; }` (matches _z_sys_net_endpoint_t)
#[repr(C)]
#[derive(Clone, Copy)]
struct Endpoint {
    _addr: u32, // IPv4 in host byte order
    _port: u16, // port in host byte order
}

/// `struct nx_bsd_sockaddr_in` — IPv4 socket address for NetX Duo BSD.
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: InAddr,
    sin_zero: [u8; 8],
}

#[repr(C)]
struct InAddr {
    s_addr: u32,
}

// Socket constants (NetX Duo BSD)
const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const SOCK_DGRAM: c_int = 2;
const IPPROTO_TCP: c_int = 6;
const IPPROTO_UDP: c_int = 17;
const SOL_SOCKET: c_int = 0xFFFF;
const SO_RCVTIMEO: c_int = 0x1006;
const SO_REUSEADDR: c_int = 0x0004;

// ============================================================================
// Helpers
// ============================================================================

/// Parse "a.b.c.d" → host-byte-order u32.
fn parse_ipv4(s: *const u8) -> u32 {
    if s.is_null() {
        return 0;
    }
    let mut octets = [0u32; 4];
    let mut idx = 0usize;
    let mut val: u32 = 0;
    let mut has_digit = false;
    let mut p = s;

    loop {
        let ch = unsafe { *p };
        if ch == 0 {
            break;
        }
        if ch.is_ascii_digit() {
            val = val * 10 + (ch - b'0') as u32;
            has_digit = true;
        } else if ch == b'.' {
            if !has_digit || idx >= 3 {
                return 0;
            }
            octets[idx] = val;
            idx += 1;
            val = 0;
            has_digit = false;
        } else {
            return 0;
        }
        p = unsafe { p.add(1) };
    }

    if !has_digit || idx != 3 {
        return 0;
    }
    octets[3] = val;

    (octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]
}

fn parse_port(s: *const u8) -> u16 {
    if s.is_null() {
        return 0;
    }
    let mut val: u32 = 0;
    let mut p = s;
    loop {
        let ch = unsafe { *p };
        if ch == 0 {
            break;
        }
        if ch.is_ascii_digit() {
            val = val * 10 + (ch - b'0') as u32;
        } else {
            return 0;
        }
        p = unsafe { p.add(1) };
    }
    val as u16
}

fn ep_to_sockaddr(ep: &Endpoint) -> SockAddrIn {
    SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port: unsafe { htons(ep._port) },
        sin_addr: InAddr {
            s_addr: unsafe { htonl(ep._addr) },
        },
        sin_zero: [0; 8],
    }
}

// ============================================================================
// TCP
// ============================================================================

impl ThreadxPlatform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let addr = parse_ipv4(address);
        let p = parse_port(port);
        if addr == 0 {
            return -1;
        }
        unsafe {
            (*ep)._addr = addr;
            (*ep)._port = p;
        }
        0
    }

    pub fn tcp_free_endpoint(_ep: *mut c_void) {
        // Static storage — nothing to free
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let fd = unsafe { nx_bsd_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO (NetX BSD takes INT milliseconds, not struct timeval)
        if timeout_ms > 0 {
            let tv_ms: c_int = timeout_ms as c_int;
            unsafe {
                nx_bsd_setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    &tv_ms as *const _ as *const c_void,
                    core::mem::size_of::<c_int>() as c_int,
                );
            }
        }

        let addr = ep_to_sockaddr(rep);
        let rc = unsafe { nx_bsd_connect(fd, &addr, core::mem::size_of::<SockAddrIn>() as c_int) };
        if rc < 0 {
            unsafe {
                nx_bsd_soc_close(fd);
                (*sock)._fd = -1;
            }
            return -1;
        }

        0
    }

    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        let sock = sock as *mut Socket;
        let lep = unsafe { &*(endpoint as *const Endpoint) };

        let fd = unsafe { nx_bsd_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) };
        if fd < 0 {
            return -1;
        }

        let one: c_int = 1;
        unsafe {
            nx_bsd_setsockopt(
                fd,
                SOL_SOCKET,
                SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<c_int>() as c_int,
            );
        }

        let addr = ep_to_sockaddr(lep);
        if unsafe { nx_bsd_bind(fd, &addr, core::mem::size_of::<SockAddrIn>() as c_int) } < 0 {
            unsafe { nx_bsd_soc_close(fd) };
            return -1;
        }

        if unsafe { nx_bsd_listen(fd, 1) } < 0 {
            unsafe { nx_bsd_soc_close(fd) };
            return -1;
        }

        unsafe { (*sock)._fd = fd };
        0
    }

    pub fn tcp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        let fd = unsafe { (*sock)._fd };
        if fd >= 0 {
            unsafe {
                nx_bsd_soc_close(fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let n = unsafe { nx_bsd_recv(sock._fd, buf, len as c_int, 0) };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let mut total: usize = 0;
        while total < len {
            let n = unsafe { nx_bsd_recv(sock._fd, buf.add(total), (len - total) as c_int, 0) };
            if n <= 0 {
                return usize::MAX;
            }
            total += n as usize;
        }
        total
    }

    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let mut total: usize = 0;
        while total < len {
            let n = unsafe { nx_bsd_send(sock._fd, buf.add(total), (len - total) as c_int, 0) };
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

impl ThreadxPlatform {
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        Self::tcp_create_endpoint(ep, address, port)
    }

    pub fn udp_free_endpoint(_ep: *mut c_void) {}

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let fd = unsafe { nx_bsd_socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        if timeout_ms > 0 {
            let tv_ms: c_int = timeout_ms as c_int;
            unsafe {
                nx_bsd_setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    &tv_ms as *const _ as *const c_void,
                    core::mem::size_of::<c_int>() as c_int,
                );
            }
        }

        // Connect for send/recv (instead of sendto/recvfrom)
        let addr = ep_to_sockaddr(rep);
        if unsafe { nx_bsd_connect(fd, &addr, core::mem::size_of::<SockAddrIn>() as c_int) } < 0 {
            unsafe {
                nx_bsd_soc_close(fd);
                (*sock)._fd = -1;
            }
            return -1;
        }

        0
    }

    pub fn udp_close(sock: *mut c_void) {
        Self::tcp_close(sock);
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let n = unsafe { nx_bsd_recv(sock._fd, buf, len as c_int, 0) };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        Self::udp_read(sock, buf, len) // UDP datagram
    }

    pub fn udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let addr = ep_to_sockaddr(rep);
        let n = unsafe {
            nx_bsd_sendto(
                sock._fd,
                buf,
                len as c_int,
                0,
                &addr,
                core::mem::size_of::<SockAddrIn>() as c_int,
            )
        };
        if n <= 0 { usize::MAX } else { n as usize }
    }
}

// ============================================================================
// Socket helpers
// ============================================================================

impl ThreadxPlatform {
    pub fn socket_set_non_blocking(_sock: *const c_void) -> i8 {
        0 // NetX Duo BSD does not support fcntl/O_NONBLOCK
    }

    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        let sin = unsafe { &*(sock_in as *const Socket) };
        let sout = sock_out as *mut Socket;
        let mut addr: SockAddrIn = unsafe { core::mem::zeroed() };
        let mut addrlen: c_int = core::mem::size_of::<SockAddrIn>() as c_int;

        let fd = unsafe { nx_bsd_accept(sin._fd, &mut addr, &mut addrlen) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sout)._fd = fd };
        0
    }

    pub fn socket_close(sock: *mut c_void) {
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
        unsafe { tx_thread_sleep(1) };
        0
    }
}

// ============================================================================
// UDP multicast (stubs — not supported on ThreadX/NetX Duo)
// ============================================================================

impl ThreadxPlatform {
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
