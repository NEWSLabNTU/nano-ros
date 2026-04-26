//! Zephyr TCP/UDP networking via the Zephyr POSIX socket subsystem.
//!
//! Implements `PlatformTcp`, `PlatformUdp`, `PlatformSocketHelpers`, and
//! `PlatformUdpMulticast` for Zephyr. The symbols (`socket`, `connect`,
//! `getaddrinfo`, …) come from Zephyr's POSIX/net-socket layer and are
//! declared here via manual `extern "C"` blocks — the `libc` crate does
//! not target Zephyr.
//!
//! Replaces `zenoh-pico/src/system/zephyr/network.c` (~730 lines of C).
//! Logic mirrors `nros-platform-posix/src/net.rs`; differences are:
//!
//! - Zephyr's constants (`AF_INET=1`, `O_NONBLOCK=0x4000`, …) differ from
//!   Linux libc. See the `c` module below for the Zephyr-specific values,
//!   cross-checked against `zephyr/include/zephyr/net/socket.h` and
//!   `zephyr/include/zephyr/net/net_ip.h`.
//! - `SHUT_RDWR` is an `enum` value on Zephyr (not a `#define`). We use
//!   the numeric value `2` which matches `ZSOCK_SHUT_RDWR`.
//! - UDP multicast is stubbed (returns `-1`/`usize::MAX`) — Zephyr's
//!   zenoh-pico tests use `tcp/…` locators, not multicast scouting.
//!   Follow-up work item tracked in Phase 80.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_int, c_void};

use crate::ZephyrPlatform;
#[allow(unused_imports)]
use nros_platform_api::PlatformSleep;

// ============================================================================
// Zephyr POSIX / BSD socket API — manually-declared bindings.
// ============================================================================
//
// Values verified against:
//   zephyr/include/zephyr/net/net_ip.h       (AF_INET, PF_*, SOCK_*, IPPROTO_*)
//   zephyr/include/zephyr/net/socket.h       (SOL_SOCKET, SO_*, TCP_*, SHUT_*)
//   zephyr/include/zephyr/posix/fcntl.h      (F_GETFL, F_SETFL, O_NONBLOCK)
#[allow(non_camel_case_types)]
mod c {
    use core::ffi::{c_char, c_int, c_void};

    // Zephyr: `typedef size_t socklen_t` — 8 bytes on 64-bit native_sim,
    // 4 bytes on 32-bit ARM. Using usize matches.
    pub type socklen_t = usize;
    pub type sa_family_t = u16;
    pub type ssize_t = isize;

    // Address families (Zephyr: AF_INET = PF_INET = 1 — NOT the Linux value 2)
    pub const PF_UNSPEC: c_int = 0;

    // Socket types
    pub const SOCK_STREAM: c_int = 1;
    pub const SOCK_DGRAM: c_int = 2;

    // Protocols
    pub const IPPROTO_TCP: c_int = 6;
    pub const IPPROTO_UDP: c_int = 17;

    // Socket option levels / names (Zephyr values)
    pub const SOL_SOCKET: c_int = 1;
    pub const SO_KEEPALIVE: c_int = 9;
    pub const SO_RCVTIMEO: c_int = 20;
    pub const SO_LINGER: c_int = 13;
    pub const SO_REUSEADDR: c_int = 2;

    // IPPROTO_TCP options
    pub const TCP_NODELAY: c_int = 1;

    // IPv4 multicast options (Zephyr socket.h)
    pub const IPPROTO_IP: c_int = 0;
    pub const IP_ADD_MEMBERSHIP: c_int = 35;
    #[allow(dead_code)]
    pub const IP_DROP_MEMBERSHIP: c_int = 36;

    // INADDR_ANY in network byte order — zero matches.
    pub const INADDR_ANY_BE: u32 = 0;

    /// `struct in_addr { uint32_t s_addr; }` — IPv4 address in network
    /// byte order.
    #[repr(C)]
    pub struct in_addr {
        pub s_addr: u32,
    }

    /// `struct sockaddr_in` — IPv4 socket address.
    #[repr(C)]
    pub struct sockaddr_in {
        pub sin_family: sa_family_t,
        pub sin_port: u16,
        pub sin_addr: in_addr,
        pub sin_zero: [u8; 8],
    }

    /// `struct ip_mreqn` — IPv4 multicast membership request *with*
    /// an interface index. Zephyr's `ipv4_multicast_group()` accepts
    /// only this 12-byte form (it `optlen != sizeof(ip_mreqn)`-rejects
    /// the shorter Linux `ip_mreq`).
    #[repr(C)]
    pub struct ip_mreqn {
        pub imr_multiaddr: in_addr,
        pub imr_address: in_addr,
        pub imr_ifindex: c_int,
    }

    // Shutdown (Zephyr defines SHUT_RDWR = ZSOCK_SHUT_RDWR = 2)
    pub const SHUT_RDWR: c_int = 2;

    // fcntl commands (Zephyr posix/fcntl.h)
    pub const F_GETFL: c_int = 3;
    pub const F_SETFL: c_int = 4;
    pub const O_NONBLOCK: c_int = 0x4000;

    #[repr(C)]
    pub struct timeval {
        pub tv_sec: i64,
        pub tv_usec: i64,
    }

    #[repr(C)]
    pub struct linger {
        pub l_onoff: c_int,
        pub l_linger: c_int,
    }

    /// `struct sockaddr_storage` — opaque, 128 bytes on every Zephyr
    /// config. We only ever pass pointers to it.
    #[repr(C)]
    pub struct sockaddr_storage {
        pub ss_family: sa_family_t,
        pub _pad: [u8; 126],
    }

    /// `struct sockaddr` — same as sockaddr_storage in practice; we only
    /// pass pointers.
    #[repr(C)]
    pub struct sockaddr {
        pub sa_family: sa_family_t,
        pub sa_data: [u8; 14],
    }

    /// `DNS_MAX_NAME_SIZE + 1` from `zephyr/net/dns_resolve.h` — inline
    /// canonical-name storage in the zsock_addrinfo record.
    pub const DNS_MAX_NAME_SIZE_PLUS_1: usize = 21;

    /// Matches `struct zsock_addrinfo` in `zephyr/net/socket.h`. This is
    /// **not** the POSIX `struct addrinfo` layout — `ai_next` comes
    /// first, there's an extra `ai_eflags`, the order of `ai_addr`/
    /// `ai_canonname` is swapped, and the struct embeds storage for
    /// both (`_ai_addr`, `_ai_canonname`) so that `getaddrinfo()` does
    /// not allocate.
    ///
    /// POSIX `getaddrinfo` in `zephyr/lib/posix/options/net.c` is a thin
    /// wrapper that forwards to `zsock_getaddrinfo`, so this is the
    /// binary layout we must match.
    #[repr(C)]
    pub struct addrinfo {
        pub ai_next: *mut addrinfo,
        pub ai_flags: c_int,
        pub ai_family: c_int,
        pub ai_socktype: c_int,
        pub ai_protocol: c_int,
        pub ai_eflags: c_int,
        pub ai_addrlen: socklen_t,
        pub ai_addr: *mut sockaddr,
        pub ai_canonname: *mut c_char,
        // Internal storage — zsock_getaddrinfo fills these and sets
        // `ai_addr`/`ai_canonname` to point at them. We never touch
        // them from Rust, but their size affects the total struct
        // size when zenoh-pico allocates an `addrinfo` on the stack.
        pub _ai_addr: sockaddr,
        pub _ai_canonname: [u8; DNS_MAX_NAME_SIZE_PLUS_1],
    }

    // All socket functions use nros_zephyr_* C shim wrappers that call
    // Zephyr's zsock_* API. On native_sim, glibc's BSD socket symbols
    // (socket, connect, getaddrinfo, etc.) override Zephyr's POSIX wrappers,
    // causing ABI mismatches (e.g., POSIX addrinfo vs zsock_addrinfo layout).
    // The shims are defined in nros_platform_zephyr_shims.c.
    unsafe extern "C" {
        #[link_name = "nros_zephyr_socket"]
        pub fn socket(family: c_int, ty: c_int, proto: c_int) -> c_int;
        #[link_name = "nros_zephyr_close"]
        pub fn close(fd: c_int) -> c_int;
        #[link_name = "nros_zephyr_connect"]
        pub fn connect(fd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
        #[link_name = "nros_zephyr_bind"]
        pub fn bind(fd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
        #[link_name = "nros_zephyr_listen"]
        pub fn listen(fd: c_int, backlog: c_int) -> c_int;
        #[link_name = "nros_zephyr_accept"]
        pub fn accept(fd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int;
        #[link_name = "nros_zephyr_shutdown"]
        pub fn shutdown(fd: c_int, how: c_int) -> c_int;
        #[link_name = "nros_zephyr_setsockopt"]
        pub fn setsockopt(
            fd: c_int,
            level: c_int,
            optname: c_int,
            optval: *const c_void,
            optlen: socklen_t,
        ) -> c_int;
        #[link_name = "nros_zephyr_fcntl"]
        pub fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int;
        #[link_name = "nros_zephyr_recv"]
        pub fn recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> ssize_t;
        #[link_name = "nros_zephyr_recvfrom"]
        pub fn recvfrom(
            fd: c_int,
            buf: *mut c_void,
            len: usize,
            flags: c_int,
            addr: *mut sockaddr,
            addrlen: *mut socklen_t,
        ) -> ssize_t;
        #[link_name = "nros_zephyr_send"]
        pub fn send(fd: c_int, buf: *const c_void, len: usize, flags: c_int) -> ssize_t;
        #[link_name = "nros_zephyr_sendto"]
        pub fn sendto(
            fd: c_int,
            buf: *const c_void,
            len: usize,
            flags: c_int,
            dest: *const sockaddr,
            addrlen: socklen_t,
        ) -> ssize_t;
        #[link_name = "nros_zephyr_getaddrinfo"]
        pub fn getaddrinfo(
            node: *const c_char,
            service: *const c_char,
            hints: *const addrinfo,
            res: *mut *mut addrinfo,
        ) -> c_int;
        #[link_name = "nros_zephyr_freeaddrinfo"]
        pub fn freeaddrinfo(res: *mut addrinfo);
    }
}

use c::{addrinfo, linger, sockaddr, sockaddr_storage, socklen_t, timeval};
use core::ffi::c_char;

// ============================================================================
// Struct layouts matching zenoh-pico's zephyr.h
// ============================================================================
//
// Zephyr's _z_sys_net_socket_t is `{ int _fd | const struct device *_serial }`
// packed in a union. We only use the _fd side here (serial link is
// disabled for zenoh-pico on Zephyr). `_z_sys_net_endpoint_t` is
// `{ struct addrinfo *_iptcp }`.

#[repr(C)]
struct Socket {
    _fd: core::ffi::c_int,
}

#[repr(C)]
struct Endpoint {
    _iptcp: *mut addrinfo,
}

const Z_TRANSPORT_LEASE: u32 = 10000;

// ============================================================================
// TCP
// ============================================================================

impl ZephyrPlatform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = c::PF_UNSPEC;
        hints.ai_socktype = c::SOCK_STREAM;
        hints.ai_protocol = c::IPPROTO_TCP;

        let ret = unsafe {
            c::getaddrinfo(
                address as *const c_char,
                port as *const c_char,
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
                c::freeaddrinfo((*ep)._iptcp);
                (*ep)._iptcp = core::ptr::null_mut();
            }
        }
    }

    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { c::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_RCVTIMEO — non-fatal on Zephyr (matches zenoh-pico's
        // zephyr/network.c behavior: logs the error but continues).
        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as i64,
            tv_usec: ((timeout_ms % 1000) * 1000) as i64,
        };
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as socklen_t,
            );
        }

        let one: core::ffi::c_int = 1;
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
            c::setsockopt(
                fd,
                c::IPPROTO_TCP,
                c::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
        }

        let ling = linger {
            l_onoff: 1,
            l_linger: (Z_TRANSPORT_LEASE / 1000) as core::ffi::c_int,
        };
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_LINGER,
                &ling as *const _ as *const c_void,
                core::mem::size_of::<linger>() as socklen_t,
            );
        }

        // Connect — iterate through addrinfo list
        let mut it = rep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            let ret = unsafe { c::connect(fd, ai.ai_addr, ai.ai_addrlen) };
            if ret == 0 {
                return 0;
            }
            it = ai.ai_next;
        }

        unsafe {
            c::close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        let sock = sock as *mut Socket;
        let lep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*lep._iptcp };
        let fd = unsafe { c::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        let one: core::ffi::c_int = 1;
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
            c::setsockopt(
                fd,
                c::IPPROTO_TCP,
                c::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
        }

        let mut it = lep._iptcp;
        while !it.is_null() {
            let ai = unsafe { &*it };
            if unsafe { c::bind(fd, ai.ai_addr, ai.ai_addrlen) } == 0
                && unsafe { c::listen(fd, 128) } == 0
            {
                return 0;
            }
            it = ai.ai_next;
        }

        unsafe {
            c::close(fd);
            (*sock)._fd = -1;
        }
        -1
    }

    pub fn tcp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        unsafe {
            if (*sock)._fd >= 0 {
                c::shutdown((*sock)._fd, c::SHUT_RDWR);
                c::close((*sock)._fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let ret = unsafe { c::recv(sock._fd, buf as *mut c_void, len, 0) };
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
        // Zephyr doesn't define MSG_NOSIGNAL — socket writes on a closed
        // peer simply return an error instead of raising SIGPIPE.
        let ret = unsafe { c::send(sock._fd, buf as *const c_void, len, 0) };
        if ret < 0 { usize::MAX } else { ret as usize }
    }
}

// ============================================================================
// UDP unicast
// ============================================================================

impl ZephyrPlatform {
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        let ep = ep as *mut Endpoint;
        let mut hints: addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_family = c::PF_UNSPEC;
        hints.ai_socktype = c::SOCK_DGRAM;
        hints.ai_protocol = c::IPPROTO_UDP;

        let ret = unsafe {
            c::getaddrinfo(
                address as *const c_char,
                port as *const c_char,
                &hints,
                &mut (*ep)._iptcp,
            )
        };
        if ret != 0 { -1 } else { 0 }
    }

    pub fn udp_free_endpoint(ep: *mut c_void) {
        Self::tcp_free_endpoint(ep);
    }

    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };

        let ai = unsafe { &*rep._iptcp };
        let fd = unsafe { c::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as i64,
            tv_usec: ((timeout_ms % 1000) * 1000) as i64,
        };
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as socklen_t,
            );
        }
        0
    }

    /// Phase 71.21 — bind a UDP socket for inbound use via Zephyr's
    /// POSIX socket shim.
    pub fn udp_listen(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        let ai = unsafe { &*rep._iptcp };

        let fd = unsafe { c::socket(ai.ai_family, ai.ai_socktype, ai.ai_protocol) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        let one: core::ffi::c_int = 1;
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
        }

        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as i64,
            tv_usec: ((timeout_ms % 1000) * 1000) as i64,
        };
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as socklen_t,
            );
        }

        if unsafe { c::bind(fd, ai.ai_addr, ai.ai_addrlen) } < 0 {
            unsafe { c::close(fd) };
            unsafe { (*sock)._fd = -1 };
            return -1;
        }
        0
    }

    pub fn udp_close(sock: *mut c_void) {
        let sock = sock as *mut Socket;
        unsafe {
            if (*sock)._fd >= 0 {
                c::close((*sock)._fd);
                (*sock)._fd = -1;
            }
        }
    }

    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let mut raddr: sockaddr_storage = unsafe { core::mem::zeroed() };
        let mut addrlen: socklen_t = core::mem::size_of::<sockaddr_storage>() as socklen_t;
        let ret = unsafe {
            c::recvfrom(
                sock._fd,
                buf as *mut c_void,
                len,
                0,
                &mut raddr as *mut _ as *mut sockaddr,
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
            c::sendto(
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
        // forever". Cooperative DDS recv loops (Phase 71.2) call this
        // with `0` to mean non-blocking; honour that via fcntl.
        if timeout_ms == 0 {
            unsafe {
                let flags = c::fcntl(sock._fd, c::F_GETFL, 0);
                if flags >= 0 {
                    c::fcntl(sock._fd, c::F_SETFL, flags | c::O_NONBLOCK);
                }
            }
            return;
        }
        let tv = timeval {
            tv_sec: (timeout_ms / 1000) as i64,
            tv_usec: ((timeout_ms % 1000) * 1000) as i64,
        };
        unsafe {
            c::setsockopt(
                sock._fd,
                c::SOL_SOCKET,
                c::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as socklen_t,
            );
        }
    }
}

// ============================================================================
// Socket helpers
// ============================================================================

impl ZephyrPlatform {
    pub fn socket_set_non_blocking(sock: *const c_void) -> i8 {
        let sock = unsafe { &*(sock as *const Socket) };
        unsafe {
            let flags = c::fcntl(sock._fd, c::F_GETFL, 0);
            if flags == -1 {
                return -1;
            }
            if c::fcntl(sock._fd, c::F_SETFL, flags | c::O_NONBLOCK) == -1 {
                return -1;
            }
        }
        0
    }

    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        let sock_in = unsafe { &*(sock_in as *const Socket) };
        let sock_out = sock_out as *mut Socket;

        let mut naddr: sockaddr_storage = unsafe { core::mem::zeroed() };
        let mut nlen: socklen_t = core::mem::size_of::<sockaddr_storage>() as socklen_t;

        let con = unsafe {
            c::accept(
                sock_in._fd,
                &mut naddr as *mut _ as *mut sockaddr,
                &mut nlen,
            )
        };
        if con < 0 {
            return -1;
        }

        let one: core::ffi::c_int = 1;
        unsafe {
            let tv = timeval {
                tv_sec: 10,
                tv_usec: 0,
            };
            c::setsockopt(
                con,
                c::SOL_SOCKET,
                c::SO_RCVTIMEO,
                &tv as *const _ as *const c_void,
                core::mem::size_of::<timeval>() as socklen_t,
            );
            c::setsockopt(
                con,
                c::SOL_SOCKET,
                c::SO_KEEPALIVE,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
            c::setsockopt(
                con,
                c::IPPROTO_TCP,
                c::TCP_NODELAY,
                &one as *const _ as *const c_void,
                core::mem::size_of::<core::ffi::c_int>() as socklen_t,
            );
            (*sock_out)._fd = con;
        }
        0
    }

    pub fn socket_close(sock: *mut c_void) {
        Self::tcp_close(sock);
    }

    pub fn socket_wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
        // Phase 77.22: delegate to `PlatformYield::yield_now()` (`k_yield`)
        // instead of `k_msleep(1)`. The caller just needs to run the
        // scheduler, not actually sleep.
        use nros_platform_api::PlatformYield;
        <Self as PlatformYield>::yield_now();
        0
    }
}

// ============================================================================
// UDP multicast — IPv4 SPDP discovery for Phase 71 dust-dds
// ============================================================================
//
// `mcast_open`/`mcast_read_exact` are still unused by both zenoh-pico
// (which uses TCP locators on Zephyr) and dust-dds (which only needs
// `mcast_listen`/`mcast_read`/`mcast_send` for SPDP), so they remain
// stubs. Phase 71.8 wires `mcast_listen` over the host kernel via
// NSOS — see comments below for the bind+IP_ADD_MEMBERSHIP sequence.

impl ZephyrPlatform {
    pub fn mcast_open(
        _sock: *mut c_void,
        _endpoint: *const c_void,
        _lep: *mut c_void,
        _timeout_ms: u32,
        _iface: *const u8,
    ) -> i8 {
        -1
    }

    /// Parse a NUL-terminated IPv4 dotted-quad address string ("a.b.c.d")
    /// into a `u32` in **network byte order** suitable for
    /// `in_addr.s_addr`. Returns `None` on malformed input.
    fn parse_ipv4_be(addr: *const u8) -> Option<u32> {
        if addr.is_null() {
            return None;
        }
        let mut octets = [0u8; 4];
        let mut idx = 0;
        let mut acc: u32 = 0;
        let mut have_digit = false;
        let mut p = addr;
        loop {
            let b = unsafe { *p };
            p = unsafe { p.add(1) };
            match b {
                b'0'..=b'9' => {
                    acc = acc * 10 + (b - b'0') as u32;
                    if acc > 255 {
                        return None;
                    }
                    have_digit = true;
                }
                b'.' | 0 => {
                    if !have_digit {
                        return None;
                    }
                    if idx >= 4 {
                        return None;
                    }
                    octets[idx] = acc as u8;
                    idx += 1;
                    acc = 0;
                    have_digit = false;
                    if b == 0 {
                        if idx != 4 {
                            return None;
                        }
                        // Network byte order: octets[0] is most
                        // significant. `s_addr` is stored in NBO.
                        let be = ((octets[0] as u32) << 24)
                            | ((octets[1] as u32) << 16)
                            | ((octets[2] as u32) << 8)
                            | (octets[3] as u32);
                        return Some(be.to_be());
                    }
                }
                _ => return None,
            }
        }
    }

    /// Phase 71.8 — bind a UDP socket to an IPv4 multicast group for
    /// SPDP discovery.
    ///
    /// Sequence: `socket(AF_INET, SOCK_DGRAM)` → `SO_REUSEADDR` →
    /// `bind(0.0.0.0:port)` → `setsockopt(IP_ADD_MEMBERSHIP, ip_mreq)`.
    /// Honours the cooperative non-blocking contract by translating
    /// `timeout_ms = 0` into `fcntl(O_NONBLOCK)` via the shared
    /// `udp_set_recv_timeout` helper.
    ///
    /// **NSOS (`native_sim`) limitation**: Zephyr's NSOS adapter
    /// (`zephyr/drivers/net/nsos_adapt.c::nsos_adapt_setsockopt`) only
    /// translates `SOL_SOCKET`, `IPPROTO_TCP`, and `IPPROTO_IPV6`
    /// options to the host kernel — it does **not** forward
    /// `IPPROTO_IP / IP_ADD_MEMBERSHIP`, so the membership join here
    /// always returns `EOPNOTSUPP` and `mcast_listen` returns `-1`.
    /// On real Zephyr (`qemu_cortex_m3` with native networking +
    /// IGMP) and on a patched NSOS the implementation is correct;
    /// the SPDP-on-`native_sim`-via-NSOS gap is tracked under Phase
    /// 71.8 as a Zephyr upstream item.
    pub fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        _iface: *const u8,
        join: *const u8,
    ) -> i8 {
        let sock = sock as *mut Socket;
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        if rep._iptcp.is_null() {
            return -1;
        }
        let ai = unsafe { &*rep._iptcp };
        if ai.ai_addr.is_null() {
            return -1;
        }

        // Pull the parsed sin_port out of the resolved sockaddr.
        // We intentionally bind to INADDR_ANY rather than the
        // multicast group itself: many Linux/Zephyr kernels only
        // deliver multicast traffic to a socket bound to INADDR_ANY
        // (or the iface address), not the group.
        let port_be = unsafe { (*(ai.ai_addr as *const c::sockaddr_in)).sin_port };

        let fd = unsafe { c::socket(ai.ai_family, c::SOCK_DGRAM, c::IPPROTO_UDP) };
        if fd < 0 {
            return -1;
        }
        unsafe { (*sock)._fd = fd };

        // SO_REUSEADDR — multiple participants on the same host need
        // to share the SPDP multicast port without `EADDRINUSE`.
        let one: c_int = 1;
        unsafe {
            c::setsockopt(
                fd,
                c::SOL_SOCKET,
                c::SO_REUSEADDR,
                &one as *const _ as *const c_void,
                core::mem::size_of::<c_int>() as c::socklen_t,
            );
        }

        // bind(2) to 0.0.0.0:port — INADDR_ANY for the local end.
        let mut bind_addr: c::sockaddr_in = unsafe { core::mem::zeroed() };
        bind_addr.sin_family = ai.ai_family as c::sa_family_t;
        bind_addr.sin_port = port_be;
        bind_addr.sin_addr.s_addr = c::INADDR_ANY_BE;
        let bind_ret = unsafe {
            c::bind(
                fd,
                &bind_addr as *const _ as *const c::sockaddr,
                core::mem::size_of::<c::sockaddr_in>() as c::socklen_t,
            )
        };
        if bind_ret < 0 {
            unsafe {
                c::close(fd);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Join the multicast group (IP_ADD_MEMBERSHIP).
        let group = match Self::parse_ipv4_be(join) {
            Some(v) => v,
            None => {
                unsafe {
                    c::close(fd);
                    (*sock)._fd = -1;
                }
                return -1;
            }
        };
        let mreq = c::ip_mreqn {
            imr_multiaddr: c::in_addr { s_addr: group },
            imr_address: c::in_addr { s_addr: c::INADDR_ANY_BE },
            imr_ifindex: 0,
        };
        let join_ret = unsafe {
            c::setsockopt(
                fd,
                c::IPPROTO_IP,
                c::IP_ADD_MEMBERSHIP,
                &mreq as *const _ as *const c_void,
                core::mem::size_of::<c::ip_mreqn>() as c::socklen_t,
            )
        };
        if join_ret < 0 {
            unsafe {
                c::close(fd);
                (*sock)._fd = -1;
            }
            return -1;
        }

        // Honour the cooperative non-blocking contract.
        Self::udp_set_recv_timeout(sock as *const c_void, timeout_ms);
        0
    }

    pub fn mcast_close(
        _sockrecv: *mut c_void,
        _socksend: *mut c_void,
        _rep: *const c_void,
        _lep: *const c_void,
    ) {
        // Membership drop + close handled by the higher-level
        // tear-down path that owns the OpaqueSocket; doing it here
        // would double-free under dust-dds's RAII teardown. Left as a
        // no-op intentionally for parity with the zenoh-pico stub.
    }

    pub fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        _lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let mut sender_storage: c::sockaddr_storage = unsafe { core::mem::zeroed() };
        let mut sender_len: c::socklen_t =
            core::mem::size_of::<c::sockaddr_storage>() as c::socklen_t;
        let n = unsafe {
            c::recvfrom(
                sock._fd,
                buf as *mut c_void,
                len,
                0,
                &mut sender_storage as *mut _ as *mut c::sockaddr,
                &mut sender_len,
            )
        };
        if n <= 0 {
            return usize::MAX;
        }
        // Best-effort: copy the parsed sender sockaddr into the
        // caller's `addr` buffer (Endpoint = { addrinfo* _iptcp }).
        // Most callers (cooperative recv loops) ignore this — leave
        // their pre-zeroed buffer alone.
        let _ = addr;
        n as usize
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
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        let sock = unsafe { &*(sock as *const Socket) };
        let rep = unsafe { &*(endpoint as *const Endpoint) };
        if rep._iptcp.is_null() {
            return usize::MAX;
        }
        let ai = unsafe { &*rep._iptcp };
        let n = unsafe {
            c::sendto(
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

impl nros_platform_api::PlatformTcp for ZephyrPlatform {
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

impl nros_platform_api::PlatformUdp for ZephyrPlatform {
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

impl nros_platform_api::PlatformSocketHelpers for ZephyrPlatform {
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

impl nros_platform_api::PlatformUdpMulticast for ZephyrPlatform {
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
