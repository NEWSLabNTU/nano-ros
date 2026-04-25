//! POSIX TCP/UDP/multicast networking via libc.
//!
//! Implements `PlatformTcp`, `PlatformUdp`, `PlatformUdpMulticast`,
//! and `PlatformSocketHelpers` for POSIX systems using BSD sockets.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::c_void;
use core::ptr;

use crate::PosixPlatform;

// ============================================================================
// Struct layouts (must match zenoh-pico's unix.h _z_sys_net_socket_t / _z_sys_net_endpoint_t)
// ============================================================================

/// Socket: `{ int _fd; void* _tls_sock; }`
#[repr(C)]
struct Socket {
    _fd: libc::c_int,
    _tls_sock: *mut c_void,
}

/// Endpoint: `{ struct addrinfo* _iptcp; }`
#[repr(C)]
struct Endpoint {
    _iptcp: *mut libc::addrinfo,
}

// Constants
const Z_TRANSPORT_LEASE: u32 = 10000; // ms (default zenoh-pico lease)

// ============================================================================
// TCP
// ============================================================================

impl PosixPlatform {
    // -- TCP --

    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: libc::addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = libc::PF_UNSPEC;
        hints.ai_socktype = libc::SOCK_STREAM;
        hints.ai_protocol = libc::IPPROTO_TCP;

        let ret = unsafe {
            libc::getaddrinfo(
                address as *const libc::c_char,
                port as *const libc::c_char,
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
                libc::freeaddrinfo((*ep)._iptcp);
            }
        }
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        } < 0
        {
            unsafe { libc::close(fd) };
            return -1;
        }

        // SO_KEEPALIVE
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // TCP_NODELAY
        unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // SO_LINGER
        let ling = libc::linger {
            l_onoff: 1,
            l_linger: (Z_TRANSPORT_LEASE / 1000) as libc::c_int,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_LINGER,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<libc::linger>() as libc::socklen_t,
            );
        }

        // Connect — iterate through addrinfo list
        let mut it = rep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { libc::connect(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                return 0; // Connected
            }
            it = ai.ai_next;
        }

        // All connect attempts failed
        unsafe {
            libc::close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        let sock = sock as *mut Socket;
        let lep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*lep._iptcp };
        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_REUSEADDR
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // SO_KEEPALIVE + TCP_NODELAY + SO_LINGER (same as open)
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        let ling = libc::linger {
            l_onoff: 1,
            l_linger: (Z_TRANSPORT_LEASE / 1000) as libc::c_int,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_LINGER,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<libc::linger>() as libc::socklen_t,
            );
        }

        // Bind + listen — iterate through addrinfo list
        let mut it = lep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { libc::bind(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                let ret = unsafe { libc::listen(fd, 128) };
                if ret == 0 {
                    return 0;
                }
            }
            it = ai.ai_next;
        }

        unsafe {
            libc::close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn tcp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        unsafe {
            if (*sock)._fd >= 0 {
                libc::shutdown((*sock)._fd, libc::SHUT_RDWR);
                libc::close((*sock)._fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let ret = unsafe { libc::recv(sock._fd, buf as *mut c_void, len, 0) };
        if ret < 0 { usize::MAX } else { ret as usize }
    }

    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let mut n: usize = 0;
        while n < len {
            let r = Self::tcp_read(sock, unsafe { buf.add(n) }, len - n);
            if r == usize::MAX {
                return usize::MAX;
            }
            if r == 0 {
                return 0;
            }
            n += r;
        }
        n
    }

    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        #[cfg(target_os = "linux")]
        let flags = libc::MSG_NOSIGNAL;
        #[cfg(not(target_os = "linux"))]
        let flags = 0;
        let ret = unsafe { libc::send(sock._fd, buf as *const c_void, len, flags) };
        if ret < 0 { usize::MAX } else { ret as usize }
    }

    // -- UDP unicast --

    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: libc::addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = libc::PF_UNSPEC;
        hints.ai_socktype = libc::SOCK_DGRAM;
        hints.ai_protocol = libc::IPPROTO_UDP;

        let ret = unsafe {
            libc::getaddrinfo(
                address as *const libc::c_char,
                port as *const libc::c_char,
                &hints,
                &mut (*ep)._iptcp,
            )
        };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn udp_free_endpoint(ep: *mut c_void) {
        Self::tcp_free_endpoint(ep); // Same: freeaddrinfo
    }

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        } < 0
        {
            unsafe {
                libc::close(fd);
                (*sock)._fd = -1;
            }
            return -1;
        }
        0
    }

    pub fn udp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        unsafe {
            if (*sock)._fd >= 0 {
                libc::close((*sock)._fd);
                (*sock)._fd = -1;
            }
        }
    }

    /// Phase 71.21 — bind a UDP socket for inbound use.
    ///
    /// Differs from `udp_open` in that this calls `bind(2)` after
    /// `socket(2)`. `udp_open` is for zenoh-pico-style outbound-only
    /// usage where the local port is ephemeral and never observed
    /// by peers; DDS needs the local port to be the deterministic
    /// RTPS PSM port so peers can address replies back.
    pub fn udp_listen(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };

        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_REUSEADDR — multiple participants on the same host need
        // to share metatraffic ports without `EADDRINUSE`.
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // SO_RCVTIMEO — `timeout_ms = 0` requests a non-blocking recv,
        // matching the contract `transport_nros::unicast_recv_loop`
        // expects (`set_recv_timeout(0)` first, then loop).
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }

        // bind(2)
        let mut it = rep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { libc::bind(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                return 0;
            }
            it = ai.ai_next;
        }

        unsafe {
            libc::close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let mut raddr: libc::sockaddr_storage = unsafe { core::mem::zeroed() };
        let mut addrlen: libc::socklen_t =
            core::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        let ret = unsafe {
            libc::recvfrom(
                sock._fd,
                buf as *mut c_void,
                len,
                0,
                &mut raddr as *mut _ as *mut libc::sockaddr,
                &mut addrlen,
            )
        };
        if ret < 0 { usize::MAX } else { ret as usize }
    }

    pub fn udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let mut n: usize = 0;
        while n < len {
            let r = Self::udp_read(sock, unsafe { buf.add(n) }, len - n);
            if r == usize::MAX {
                return usize::MAX;
            }
            if r == 0 {
                return 0;
            }
            n += r;
        }
        n
    }

    pub fn udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };
        let ret = unsafe {
            libc::sendto(
                sock._fd,
                buf as *const c_void,
                len,
                0,
                ai.ai_addr,
                ai.ai_addrlen,
            )
        };
        if ret < 0 { usize::MAX } else { ret as usize }
    }

    pub fn udp_set_recv_timeout(sock: *const c_void, timeout_ms: u32) {
        let sock = unsafe { &*(sock as *const Socket) };
        // POSIX `SO_RCVTIMEO` with `{0, 0}` means "no timeout — block
        // forever", which is the opposite of what callers expect when
        // they pass `timeout_ms = 0`. The cooperative DDS recv loops
        // (Phase 71.2) call this with `0` to request non-blocking
        // reads — translate that into an `fcntl(O_NONBLOCK)` instead.
        if timeout_ms == 0 {
            unsafe {
                let flags = libc::fcntl(sock._fd, libc::F_GETFL, 0);
                if flags >= 0 {
                    libc::fcntl(sock._fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
            return;
        }
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        unsafe {
            libc::setsockopt(
                sock._fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }
    }

    // -- Socket helpers --

    pub fn socket_set_non_blocking(sock: *const c_void) -> i8 {
        let sock = unsafe { &*(sock as *const Socket) };
        unsafe {
            let flags = libc::fcntl(sock._fd, libc::F_GETFL, 0);
            if flags == -1 {
                return -1;
            }
            if libc::fcntl(sock._fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
                return -1;
            }
        }
        0
    }

    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        let sock_in = unsafe { &*(sock_in as *const Socket) };
        let sock_out = sock_out as *mut Socket;

        let mut naddr: libc::sockaddr_storage = unsafe { core::mem::zeroed() };
        let mut nlen: libc::socklen_t =
            core::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        let con = unsafe {
            libc::accept(
                sock_in._fd,
                &mut naddr as *mut _ as *mut libc::sockaddr,
                &mut nlen,
            )
        };
        if con < 0 {
            return -1;
        }

        // Set socket options on accepted connection
        let one: libc::c_int = 1;
        unsafe {
            let tv = libc::timeval {
                tv_sec: 10, // Z_CONFIG_SOCKET_TIMEOUT / 1000
                tv_usec: 0,
            };
            libc::setsockopt(
                con,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
            libc::setsockopt(
                con,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                con,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            (*sock_out)._fd = con;
            (*sock_out)._tls_sock = ptr::null_mut();
        }
        0
    }

    pub fn socket_close(sock: *mut c_void) {
        // Delegate to tcp_close (same shutdown + close logic)
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8 {
        // Phase 77.22: delegate to `PlatformYield::yield_now()`. The
        // caller isn't waiting for I/O readability — the background
        // read task handles that — it just needs the scheduler to
        // run. `sched_yield(2)` is the smallest primitive that
        // satisfies the intent.
        let _ = (peers, mutex);
        use nros_platform_api::PlatformYield;
        <Self as PlatformYield>::yield_now();
        0
    }
}

// ============================================================================
// UDP multicast
// ============================================================================

/// _z_slice_t compatible layout for address return.
#[repr(C)]
pub struct ZSlice {
    pub len: usize,
    pub start: *const u8,
    pub _deleter: *mut c_void,
    pub _context: *mut c_void,
}

/// Get the local interface address for a given interface name and address family.
/// Returns the sockaddr and its length, or (null, 0) on failure.
unsafe fn get_ip_from_iface(
    iface: *const u8,
    sa_family: libc::c_int,
) -> (*mut libc::sockaddr, libc::socklen_t) {
    let mut ifaddrs: *mut libc::ifaddrs = ptr::null_mut();
    if libc::getifaddrs(&mut ifaddrs) != 0 {
        return (ptr::null_mut(), 0);
    }

    let mut result: *mut libc::sockaddr = ptr::null_mut();
    let mut addrlen: libc::socklen_t = 0;

    let mut tmp = ifaddrs;
    while !tmp.is_null() {
        let ifa = &*tmp;
        if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as libc::c_int == sa_family {
            // Compare interface name
            let name_matches = if !iface.is_null() {
                let mut i = 0;
                loop {
                    let a = *ifa.ifa_name.add(i) as u8;
                    let b = *iface.add(i);
                    if a != b {
                        break false;
                    }
                    if a == 0 {
                        break true;
                    }
                    i += 1;
                }
            } else {
                false
            };

            if name_matches {
                if sa_family == libc::AF_INET {
                    let size = core::mem::size_of::<libc::sockaddr_in>();
                    result = libc::malloc(size) as *mut libc::sockaddr;
                    if !result.is_null() {
                        libc::memcpy(result as *mut c_void, ifa.ifa_addr as *const c_void, size);
                        addrlen = size as libc::socklen_t;
                    }
                } else if sa_family == libc::AF_INET6 {
                    let size = core::mem::size_of::<libc::sockaddr_in6>();
                    result = libc::malloc(size) as *mut libc::sockaddr;
                    if !result.is_null() {
                        libc::memcpy(result as *mut c_void, ifa.ifa_addr as *const c_void, size);
                        addrlen = size as libc::socklen_t;
                    }
                }
                if !result.is_null() {
                    break;
                }
            }
        }
        tmp = ifa.ifa_next;
    }

    libc::freeifaddrs(ifaddrs);
    (result, addrlen)
}

impl PosixPlatform {
    pub fn mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        iface: *const u8,
    ) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let lep = lep as *mut Endpoint;

        let ai = unsafe { &*rep._iptcp };
        let (lsockaddr, addrlen) = unsafe { get_ip_from_iface(iface, ai.ai_family) };
        if lsockaddr.is_null() {
            return -1;
        }

        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            unsafe { libc::free(lsockaddr as *mut c_void) };
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        } < 0
        {
            unsafe {
                libc::close(fd);
                libc::free(lsockaddr as *mut c_void);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Bind to local address
        if unsafe { libc::bind(fd, lsockaddr, addrlen) } < 0 {
            unsafe {
                libc::close(fd);
                libc::free(lsockaddr as *mut c_void);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Get assigned port
        let mut bound_addrlen = addrlen;
        unsafe { libc::getsockname(fd, lsockaddr, &mut bound_addrlen) };

        // Set IP_MULTICAST_IF
        unsafe {
            if (*lsockaddr).sa_family as libc::c_int == libc::AF_INET {
                let addr = &(*(lsockaddr as *const libc::sockaddr_in)).sin_addr;
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_MULTICAST_IF,
                    addr as *const _ as *const c_void,
                    core::mem::size_of::<libc::in_addr>() as libc::socklen_t,
                );
            } else if (*lsockaddr).sa_family as libc::c_int == libc::AF_INET6 {
                let ifindex = libc::if_nametoindex(iface as *const libc::c_char) as libc::c_int;
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_MULTICAST_IF,
                    &ifindex as *const _ as *const c_void,
                    core::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }

        // Create local endpoint addrinfo
        let laddr =
            unsafe { libc::malloc(core::mem::size_of::<libc::addrinfo>()) as *mut libc::addrinfo };
        if laddr.is_null() {
            unsafe {
                libc::close(fd);
                libc::free(lsockaddr as *mut c_void);
                (*sock)._fd = -1;
            }
            return -1;
        }
        unsafe {
            (*laddr).ai_flags = 0;
            (*laddr).ai_family = ai.ai_family;
            (*laddr).ai_socktype = ai.ai_socktype;
            (*laddr).ai_protocol = ai.ai_protocol;
            (*laddr).ai_addrlen = addrlen;
            (*laddr).ai_addr = lsockaddr;
            (*laddr).ai_canonname = ptr::null_mut();
            (*laddr).ai_next = ptr::null_mut();
            (*lep)._iptcp = laddr;
        }
        0
    }

    pub fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        iface: *const u8,
        join: *const u8,
    ) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let (lsockaddr, _addrlen) = unsafe { get_ip_from_iface(iface, ai.ai_family) };
        if lsockaddr.is_null() {
            return -1;
        }

        let fd = unsafe { libc::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            unsafe { libc::free(lsockaddr as *mut c_void) };
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO + SO_REUSEADDR + SO_REUSEPORT
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            #[cfg(not(target_os = "windows"))]
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                &one as *const _ as *const c_void,
                core::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // Bind to INADDR_ANY with the multicast port
        let bind_ret = unsafe {
            if ai.ai_family == libc::AF_INET {
                let mut addr: libc::sockaddr_in = core::mem::zeroed();
                addr.sin_family = libc::AF_INET as libc::sa_family_t;
                addr.sin_port = (*(ai.ai_addr as *const libc::sockaddr_in)).sin_port;
                addr.sin_addr.s_addr = libc::INADDR_ANY.to_be();
                libc::bind(
                    fd,
                    &addr as *const _ as *const libc::sockaddr,
                    core::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                )
            } else {
                let mut addr: libc::sockaddr_in6 = core::mem::zeroed();
                addr.sin6_family = libc::AF_INET6 as libc::sa_family_t;
                addr.sin6_port = (*(ai.ai_addr as *const libc::sockaddr_in6)).sin6_port;
                libc::bind(
                    fd,
                    &addr as *const _ as *const libc::sockaddr,
                    core::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
                )
            }
        };
        if bind_ret < 0 {
            unsafe {
                libc::close(fd);
                libc::free(lsockaddr as *mut c_void);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Join multicast group
        let join_ret = unsafe {
            if ai.ai_family == libc::AF_INET {
                let mut mreq: libc::ip_mreq = core::mem::zeroed();
                mreq.imr_multiaddr = (*(ai.ai_addr as *const libc::sockaddr_in)).sin_addr;
                mreq.imr_interface = (*(lsockaddr as *const libc::sockaddr_in)).sin_addr;
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_ADD_MEMBERSHIP,
                    &mreq as *const _ as *const c_void,
                    core::mem::size_of::<libc::ip_mreq>() as libc::socklen_t,
                )
            } else {
                let mut mreq: libc::ipv6_mreq = core::mem::zeroed();
                mreq.ipv6mr_multiaddr = (*(ai.ai_addr as *const libc::sockaddr_in6)).sin6_addr;
                mreq.ipv6mr_interface = libc::if_nametoindex(iface as *const libc::c_char);
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_ADD_MEMBERSHIP,
                    &mreq as *const _ as *const c_void,
                    core::mem::size_of::<libc::ipv6_mreq>() as libc::socklen_t,
                )
            }
        };
        if join_ret < 0 {
            unsafe {
                libc::close(fd);
                libc::free(lsockaddr as *mut c_void);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Join additional groups (pipe-separated list)
        if !join.is_null() {
            // Skip for now — additional group join is rarely used
        }

        unsafe { libc::free(lsockaddr as *mut c_void) };
        0
    }

    pub fn mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        rep: *const c_void,
        lep: *const c_void,
    ) {
        let sockrecv = sockrecv as *mut Socket;
        let socksend = socksend as *mut Socket;
        let rep = unsafe { &*(rep as *const Endpoint) };
        let lep_ep = lep as *const Endpoint;

        // Drop membership
        unsafe {
            if (*sockrecv)._fd >= 0 && !rep._iptcp.is_null() {
                let ai = &*rep._iptcp;
                if ai.ai_family == libc::AF_INET {
                    let mut mreq: libc::ip_mreq = core::mem::zeroed();
                    mreq.imr_multiaddr = (*(ai.ai_addr as *const libc::sockaddr_in)).sin_addr;
                    mreq.imr_interface.s_addr = libc::INADDR_ANY.to_be();
                    libc::setsockopt(
                        (*sockrecv)._fd,
                        libc::IPPROTO_IP,
                        libc::IP_DROP_MEMBERSHIP,
                        &mreq as *const _ as *const c_void,
                        core::mem::size_of::<libc::ip_mreq>() as libc::socklen_t,
                    );
                } else if ai.ai_family == libc::AF_INET6 {
                    let mut mreq: libc::ipv6_mreq = core::mem::zeroed();
                    mreq.ipv6mr_multiaddr = (*(ai.ai_addr as *const libc::sockaddr_in6)).sin6_addr;
                    libc::setsockopt(
                        (*sockrecv)._fd,
                        libc::IPPROTO_IPV6,
                        libc::IPV6_DROP_MEMBERSHIP,
                        &mreq as *const _ as *const c_void,
                        core::mem::size_of::<libc::ipv6_mreq>() as libc::socklen_t,
                    );
                }
            }

            // Free lep's addrinfo + sockaddr
            if !lep_ep.is_null() && !(*lep_ep)._iptcp.is_null() {
                let laddr = (*lep_ep)._iptcp;
                if !(*laddr).ai_addr.is_null() {
                    libc::free((*laddr).ai_addr as *mut c_void);
                }
                libc::free(laddr as *mut c_void);
            }

            // Close sockets
            if (*sockrecv)._fd >= 0 {
                libc::close((*sockrecv)._fd);
                (*sockrecv)._fd = -1;
            }
            if (*socksend)._fd >= 0 {
                libc::close((*socksend)._fd);
                (*socksend)._fd = -1;
            }
        }
    }

    pub fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void, // *mut ZSlice
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let lep = unsafe { &*(lep as *const Endpoint) };
        let ai = unsafe { &*lep._iptcp };

        loop {
            let mut raddr: libc::sockaddr_storage = unsafe { core::mem::zeroed() };
            let mut replen: libc::socklen_t =
                core::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let rb = unsafe {
                libc::recvfrom(
                    sock._fd,
                    buf as *mut c_void,
                    len,
                    0,
                    &mut raddr as *mut _ as *mut libc::sockaddr,
                    &mut replen,
                )
            };
            if rb < 0 {
                return usize::MAX;
            }

            // Filter out loopback: skip if source == local
            let is_loopback = unsafe {
                if ai.ai_family == libc::AF_INET {
                    let local = &*(ai.ai_addr as *const libc::sockaddr_in);
                    let remote = &*(&raddr as *const _ as *const libc::sockaddr_in);
                    local.sin_port == remote.sin_port
                        && local.sin_addr.s_addr == remote.sin_addr.s_addr
                } else if ai.ai_family == libc::AF_INET6 {
                    let local = &*(ai.ai_addr as *const libc::sockaddr_in6);
                    let remote = &*(&raddr as *const _ as *const libc::sockaddr_in6);
                    local.sin6_port == remote.sin6_port
                        && local.sin6_addr.s6_addr == remote.sin6_addr.s6_addr
                } else {
                    true // skip unknown families
                }
            };

            if !is_loopback {
                // Write sender address to addr slice if requested
                if !addr.is_null() {
                    let slice = unsafe { &mut *(addr as *mut ZSlice) };
                    unsafe {
                        if ai.ai_family == libc::AF_INET {
                            let remote = &*(&raddr as *const _ as *const libc::sockaddr_in);
                            let ip_size = core::mem::size_of::<libc::in_addr_t>();
                            let port_size = core::mem::size_of::<libc::in_port_t>();
                            if slice.len >= ip_size + port_size {
                                slice.len = ip_size + port_size;
                                core::ptr::copy_nonoverlapping(
                                    &remote.sin_addr.s_addr as *const _ as *const u8,
                                    slice.start as *mut u8,
                                    ip_size,
                                );
                                core::ptr::copy_nonoverlapping(
                                    &remote.sin_port as *const _ as *const u8,
                                    (slice.start as *mut u8).add(ip_size),
                                    port_size,
                                );
                            }
                        } else if ai.ai_family == libc::AF_INET6 {
                            let remote = &*(&raddr as *const _ as *const libc::sockaddr_in6);
                            let ip_size = core::mem::size_of::<libc::in6_addr>();
                            let port_size = core::mem::size_of::<libc::in_port_t>();
                            if slice.len >= ip_size + port_size {
                                slice.len = ip_size + port_size;
                                core::ptr::copy_nonoverlapping(
                                    remote.sin6_addr.s6_addr.as_ptr(),
                                    slice.start as *mut u8,
                                    ip_size,
                                );
                                core::ptr::copy_nonoverlapping(
                                    &remote.sin6_port as *const _ as *const u8,
                                    (slice.start as *mut u8).add(ip_size),
                                    port_size,
                                );
                            }
                        }
                    }
                }
                return rb as usize;
            }
            // Loopback — continue reading
        }
    }

    pub fn mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        let mut n: usize = 0;
        while n < len {
            let r = Self::mcast_read(sock, unsafe { buf.add(n) }, len - n, lep, addr);
            if r == usize::MAX {
                return usize::MAX;
            }
            if r == 0 {
                return 0;
            }
            n += r;
        }
        n
    }

    pub fn mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        // Same as UDP unicast send
        Self::udp_send(sock, buf, len, endpoint)
    }
}

// ============================================================================
// Trait impls (Phase 84.F4.4)
// ============================================================================
//
// Delegate to the inherent methods above. Every shim dispatch goes through
// these traits (`<ConcretePlatform as PlatformTcp>::open(...)`) so a
// trait-method rename or addition produces a compile error here instead of
// a silent link failure.
//
// The inherent methods are kept (rather than collapsed into the trait
// bodies) so that internal `Self::tcp_read`/`Self::udp_send`/... calls in
// this file keep working without adding per-call `use` statements.

impl nros_platform_api::PlatformTcp for PosixPlatform {
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

impl nros_platform_api::PlatformUdp for PosixPlatform {
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

impl nros_platform_api::PlatformSocketHelpers for PosixPlatform {
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

impl nros_platform_api::PlatformUdpMulticast for PosixPlatform {
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
