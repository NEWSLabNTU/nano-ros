//! NuttX POSIX socket networking via nuttx-sys (bindgen).
//!
//! Uses nuttx_sys types and constants which differ from Linux:
//! SOL_SOCKET=1, SO_RCVTIMEO=10, TCP_NODELAY=16, O_NONBLOCK=64,
//! SHUT_RDWR=3, F_GETFL=2, F_SETFL=9, time_t=u64.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::c_void;

use crate::NuttxPlatform;
use nuttx_sys::*;

// ============================================================================
// C struct wrappers matching zenoh-pico's unix.h platform types
// ============================================================================

/// Socket: `{ int _fd; void* _tls_sock; }`
#[repr(C)]
struct Socket {
    _fd: core::ffi::c_int,
    _tls_sock: *mut c_void,
}

/// Endpoint: `{ struct addrinfo *_iptcp; }`
#[repr(C)]
struct Endpoint {
    _iptcp: *mut addrinfo,
}

/// Phase 71.22 — net buffer sizes (see nros-platform-posix for rationale).
pub const NET_SOCKET_SIZE: usize = core::mem::size_of::<Socket>();
pub const NET_SOCKET_ALIGN: usize = core::mem::align_of::<Socket>();
pub const NET_ENDPOINT_SIZE: usize = core::mem::size_of::<Endpoint>();
pub const NET_ENDPOINT_ALIGN: usize = core::mem::align_of::<Endpoint>();

const Z_TRANSPORT_LEASE: u32 = 10000;

// ============================================================================
// TCP
// ============================================================================

impl NuttxPlatform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = PF_UNSPEC as _;
        hints.ai_socktype = SOCK_STREAM as _;
        hints.ai_protocol = IPPROTO_TCP as _;

        let ret = unsafe {
            getaddrinfo(
                address as *const _,
                port as *const _,
                &hints,
                &mut (*ep)._iptcp,
            )
        };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn tcp_free_endpoint(ep: *mut c_void) {
        let ep = ep as *mut Endpoint;
        unsafe {
            if !(*ep)._iptcp.is_null() {
                freeaddrinfo((*ep)._iptcp);
            }
        }
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO
        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as _,
            tv_usec: ((timeout_ms % 1000) * 1000) as _,
        };
        if unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            )
        } < 0
        {
            unsafe { close(fd) };
            return -1;
        }

        // SO_KEEPALIVE
        let one: core::ffi::c_int = 1;
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_KEEPALIVE as _,
                &one as *const _ as *const c_void,
                4,
            );
        }

        // TCP_NODELAY
        unsafe {
            setsockopt(
                fd,
                IPPROTO_TCP as _,
                TCP_NODELAY as _,
                &one as *const _ as *const c_void,
                4,
            );
        }

        // SO_LINGER
        let ling = linger {
            l_onoff: 1,
            l_linger: (Z_TRANSPORT_LEASE / 1000) as _,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_LINGER as _,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<linger>() as _,
            );
        }

        // Connect
        let mut it = rep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { connect(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                return 0;
            }
            it = ai.ai_next;
        }

        unsafe {
            close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };

        let fd = unsafe { socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }

        let one: core::ffi::c_int = 1;
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_REUSEADDR as _,
                &one as *const _ as *const c_void,
                4,
            );
        }

        if unsafe { bind(fd, ai.ai_addr, ai.ai_addrlen) } < 0 {
            unsafe { close(fd) };
            return -1;
        }
        if unsafe { listen(fd, 1) } < 0 {
            unsafe { close(fd) };
            return -1;
        }

        let tv = timeval {
            tv_sec: (Z_TRANSPORT_LEASE / 1000) as _,
            tv_usec: 0 as _,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            );
        }

        unsafe { (*sock)._fd = fd };
        0
    }

    pub fn tcp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        let fd = unsafe { (*sock)._fd };
        if fd >= 0 {
            unsafe {
                shutdown(fd, SHUT_RDWR as _);
                close(fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let n = unsafe { recv(sock._fd, buf as *mut c_void, len, 0) };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let mut total: usize = 0;
        while total < len {
            let n = unsafe { recv(sock._fd, buf.add(total) as *mut c_void, len - total, 0) };
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
            let n = unsafe { send(sock._fd, buf.add(total) as *const c_void, len - total, 0) };
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

impl NuttxPlatform {
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = PF_UNSPEC as _;
        hints.ai_socktype = SOCK_DGRAM as _;
        hints.ai_protocol = IPPROTO_UDP as _;
        // RTPS only ever passes numeric IP literals — skip the
        // unconfigured DNS resolver and fail fast on parse error.
        hints.ai_flags = AI_NUMERICHOST as _;

        let ret = unsafe {
            getaddrinfo(
                address as *const _,
                port as *const _,
                &hints,
                &mut (*ep)._iptcp,
            )
        };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn udp_free_endpoint(ep: *mut c_void) {
        Self::tcp_free_endpoint(ep)
    }

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };

        let fd = unsafe { socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as _,
            tv_usec: ((timeout_ms % 1000) * 1000) as _,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            );
        }
        0
    }

    /// Phase 71.21 — bind a UDP socket for inbound use.
    pub fn udp_listen(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };

        let fd = unsafe { socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        let one: core::ffi::c_int = 1;
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_REUSEADDR as _,
                &one as *const _ as *const c_void,
                4,
            );
        }

        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as _,
            tv_usec: ((timeout_ms % 1000) * 1000) as _,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            );
        }

        if unsafe { bind(fd, ai.ai_addr, ai.ai_addrlen) } < 0 {
            unsafe { close(fd) };
            unsafe { (*sock)._fd = -1 };
            return -1;
        }
        0
    }

    pub fn udp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        let fd = unsafe { (*sock)._fd };
        if fd >= 0 {
            unsafe {
                close(fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        if sock._fd < 0 {
            return usize::MAX;
        }
        let n = unsafe {
            recvfrom(
                sock._fd,
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
        if sock._fd < 0 || rep._iptcp.is_null() {
            return usize::MAX;
        }
        let ai = unsafe { &*rep._iptcp };
        let n = unsafe {
            sendto(
                sock._fd,
                buf as *const c_void,
                len,
                0,
                ai.ai_addr,
                ai.ai_addrlen,
            )
        };
        if n <= 0 { usize::MAX } else { n as usize }
    }

    pub fn udp_set_recv_timeout(sock: *const c_void, timeout_ms: u32) {
        let sock = unsafe { &*(sock as *const Socket) };
        // POSIX `SO_RCVTIMEO` with `{0, 0}` means "no timeout — block
        // forever". Cooperative DDS recv loops (Phase 71.2) call this
        // with `0` to mean non-blocking; honour that via fcntl.
        if timeout_ms == 0 {
            unsafe {
                let flags = fcntl(sock._fd, F_GETFL as _, 0);
                if flags >= 0 {
                    fcntl(
                        sock._fd,
                        F_SETFL as _,
                        flags | O_NONBLOCK as core::ffi::c_int,
                    );
                }
            }
            return;
        }
        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as _,
            tv_usec: ((timeout_ms % 1000) * 1000) as _,
        };
        unsafe {
            setsockopt(
                sock._fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            );
        }
    }
}

// ============================================================================
// Socket helpers
// ============================================================================

impl NuttxPlatform {
    pub fn socket_set_non_blocking(sock: *const c_void) -> i8 {
        let sock = unsafe { &*(sock as *const Socket) };
        let flags = unsafe { fcntl(sock._fd, F_GETFL as _, 0) };
        if flags < 0 {
            return -1;
        }
        if unsafe {
            fcntl(
                sock._fd,
                F_SETFL as _,
                flags | O_NONBLOCK as core::ffi::c_int,
            )
        } < 0
        {
            return -1;
        }
        0
    }

    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        let sin = unsafe { &*(sock_in as *const Socket) };
        let sout = sock_out as *mut Socket;
        let mut addr: sockaddr = unsafe { core::mem::zeroed() };
        let mut addrlen: socklen_t = core::mem::size_of::<sockaddr>() as _;

        let fd = unsafe { accept(sin._fd, &mut addr, &mut addrlen) };
        if fd < 0 {
            return -1;
        }

        let tv = timeval {
            tv_sec: (Z_TRANSPORT_LEASE / 1000) as _,
            tv_usec: 0 as _,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_RCVTIMEO as _,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as _,
            );
            let one: core::ffi::c_int = 1;
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_KEEPALIVE as _,
                &one as *const _ as *const c_void,
                4,
            );
            setsockopt(
                fd,
                IPPROTO_TCP as _,
                TCP_NODELAY as _,
                &one as *const _ as *const c_void,
                4,
            );
            let ling = linger {
                l_onoff: 1,
                l_linger: (Z_TRANSPORT_LEASE / 1000) as _,
            };
            setsockopt(
                fd,
                SOL_SOCKET as _,
                SO_LINGER as _,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<linger>() as _,
            );
            (*sout)._fd = fd;
        }
        0
    }

    pub fn socket_close(sock: *mut c_void) {
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
        // Phase 77.22: delegate to `PlatformYield::yield_now()`
        // (`sched_yield(2)` on NuttX, same as POSIX).
        use nros_platform_api::PlatformYield;
        <Self as PlatformYield>::yield_now();
        0
    }
}

// ============================================================================
// UDP multicast — Phase 97.4.nuttx
// ============================================================================
//
// Same shape as the FreeRTOS / lwIP path:
//   * `mcast_open` — opens a regular UDP socket aimed at the mcast
//     group and stashes the destination endpoint in `lep` for the
//     writer's reuse on send.
//   * `mcast_listen` — `udp_listen` to bind, then setsockopt
//     `IP_ADD_MEMBERSHIP` to actually join the IGMP group, then flip
//     the socket to non-blocking when the caller asked for
//     `timeout_ms = 0` (NuttX's `SO_RCVTIMEO {0,0}` means "block
//     forever", same as lwIP).
//   * `mcast_read{,_exact}` / `mcast_send` — delegate to the unicast
//     helpers (NuttX's POSIX socket layer surfaces multicast through
//     the same recvfrom/sendto API once IGMP membership is in place).

impl NuttxPlatform {
    pub fn mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        _iface: *const u8,
    ) -> i8 {
        if !lep.is_null() && !endpoint.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    endpoint as *const u8,
                    lep as *mut u8,
                    core::mem::size_of::<Endpoint>(),
                );
            }
        }
        Self::udp_open(sock, endpoint, timeout_ms)
    }

    pub fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        _iface: *const u8,
        join: *const u8,
    ) -> i8 {
        let rc = Self::udp_listen(sock, endpoint, timeout_ms);
        if rc < 0 {
            return rc;
        }

        let group = if !join.is_null() {
            parse_ipv4_be(join)
        } else {
            0
        };
        if group == 0 {
            return rc;
        }

        let sock_ref = unsafe { &*(sock as *const Socket) };
        let fd = sock_ref._fd;
        if fd < 0 {
            return -1;
        }
        let mreq = ip_mreq {
            imr_multiaddr: in_addr { s_addr: group },
            imr_interface: in_addr { s_addr: 0 },
        };
        let setsockopt_rc = unsafe {
            setsockopt(
                fd,
                IPPROTO_IP as _,
                IP_ADD_MEMBERSHIP as _,
                &mreq as *const _ as *const c_void,
                core::mem::size_of::<ip_mreq>() as _,
            )
        };
        if setsockopt_rc < 0 {
            unsafe { close(fd) };
            return -1;
        }

        if timeout_ms == 0 {
            unsafe {
                let flags = fcntl(fd, F_GETFL as _, 0);
                if flags >= 0 {
                    fcntl(fd, F_SETFL as _, flags | O_NONBLOCK as core::ffi::c_int);
                }
            }
        }
        0
    }

    pub fn mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        _rep: *const c_void,
        _lep: *const c_void,
    ) {
        if !sockrecv.is_null() {
            Self::udp_close(sockrecv);
        }
        if !socksend.is_null() {
            Self::udp_close(socksend);
        }
    }

    pub fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        _lep: *const c_void,
        _addr: *mut c_void,
    ) -> usize {
        Self::udp_read(sock, buf, len)
    }

    pub fn mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        _lep: *const c_void,
        _addr: *mut c_void,
    ) -> usize {
        Self::udp_read_exact(sock, buf, len)
    }

    pub fn mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        Self::udp_send(sock, buf, len, endpoint)
    }
}

/// Parse a null-terminated dotted-quad ASCII string ("239.255.0.1\0")
/// into a u32 in network byte order. Returns 0 on parse failure.
fn parse_ipv4_be(s: *const u8) -> u32 {
    if s.is_null() {
        return 0;
    }
    let mut octets = [0u32; 4];
    let mut idx = 0usize;
    let mut current: u32 = 0;
    let mut has_digit = false;
    let mut p = s;
    loop {
        let ch = unsafe { *p };
        match ch {
            0 => {
                if !has_digit || idx != 3 {
                    return 0;
                }
                octets[3] = current;
                break;
            }
            b'0'..=b'9' => {
                current = current * 10 + (ch - b'0') as u32;
                if current > 255 {
                    return 0;
                }
                has_digit = true;
            }
            b'.' => {
                if !has_digit || idx >= 3 {
                    return 0;
                }
                octets[idx] = current;
                idx += 1;
                current = 0;
                has_digit = false;
            }
            _ => return 0,
        }
        p = unsafe { p.add(1) };
    }
    // network byte order = big-endian; the s_addr field on POSIX is
    // already stored in NBO so pack high-octet-first into the LSB.
    (octets[0]) | (octets[1] << 8) | (octets[2] << 16) | (octets[3] << 24)
}

// ============================================================================
// Trait impls (Phase 84.F4.4)
// ============================================================================
//
// Delegate to the inherent methods above. Every shim dispatch goes through
// these traits (`<ConcretePlatform as PlatformTcp>::open(...)`) so a
// trait-method rename or addition produces a compile error here instead of
// a silent link failure. The inherent methods are kept (rather than
// collapsed into the trait bodies) so that internal `Self::tcp_read` /
// `Self::udp_send` / ... calls in this file keep working unchanged.

impl nros_platform_api::PlatformTcp for NuttxPlatform {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        Self::tcp_create_endpoint(ep, address, port)
    }
    fn free_endpoint(ep: *mut c_void) {
        Self::tcp_free_endpoint(ep)
    }
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        Self::tcp_open(sock, endpoint, timeout_ms)
    }
    fn listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        Self::tcp_listen(sock, endpoint)
    }
    fn close(sock: *mut c_void) {
        Self::tcp_close(sock)
    }
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        Self::tcp_read(sock, buf, len)
    }
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        Self::tcp_read_exact(sock, buf, len)
    }
    fn send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        Self::tcp_send(sock, buf, len)
    }
}

impl nros_platform_api::PlatformUdp for NuttxPlatform {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        Self::udp_create_endpoint(ep, address, port)
    }
    fn free_endpoint(ep: *mut c_void) {
        Self::udp_free_endpoint(ep)
    }
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        Self::udp_open(sock, endpoint, timeout_ms)
    }
    fn close(sock: *mut c_void) {
        Self::udp_close(sock)
    }
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        Self::udp_read(sock, buf, len)
    }
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        Self::udp_read_exact(sock, buf, len)
    }
    fn send(sock: *const c_void, buf: *const u8, len: usize, endpoint: *const c_void) -> usize {
        Self::udp_send(sock, buf, len, endpoint)
    }
    fn set_recv_timeout(sock: *const c_void, timeout_ms: u32) {
        Self::udp_set_recv_timeout(sock, timeout_ms)
    }
    fn listen(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        Self::udp_listen(sock, endpoint, timeout_ms)
    }
}

impl nros_platform_api::PlatformSocketHelpers for NuttxPlatform {
    fn set_non_blocking(sock: *const c_void) -> i8 {
        Self::socket_set_non_blocking(sock)
    }
    fn accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        Self::socket_accept(sock_in, sock_out)
    }
    fn close(sock: *mut c_void) {
        Self::socket_close(sock)
    }
    fn wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8 {
        Self::socket_wait_event(peers, mutex)
    }
}

impl nros_platform_api::PlatformUdpMulticast for NuttxPlatform {
    fn mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        iface: *const u8,
    ) -> i8 {
        Self::mcast_open(sock, endpoint, lep, timeout_ms, iface)
    }
    fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        iface: *const u8,
        join: *const u8,
    ) -> i8 {
        Self::mcast_listen(sock, endpoint, timeout_ms, iface, join)
    }
    fn mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        rep: *const c_void,
        lep: *const c_void,
    ) {
        Self::mcast_close(sockrecv, socksend, rep, lep)
    }
    fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        Self::mcast_read(sock, buf, len, lep, addr)
    }
    fn mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        Self::mcast_read_exact(sock, buf, len, lep, addr)
    }
    fn mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        Self::mcast_send(sock, buf, len, endpoint)
    }
}
