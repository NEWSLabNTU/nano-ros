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

use core::ffi::c_void;

use crate::ZephyrPlatform;

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

    unsafe extern "C" {
        pub fn socket(family: c_int, ty: c_int, proto: c_int) -> c_int;
        pub fn close(fd: c_int) -> c_int;
        pub fn connect(fd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
        pub fn bind(fd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
        pub fn listen(fd: c_int, backlog: c_int) -> c_int;
        pub fn accept(fd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int;
        pub fn shutdown(fd: c_int, how: c_int) -> c_int;
        pub fn setsockopt(
            fd: c_int,
            level: c_int,
            optname: c_int,
            optval: *const c_void,
            optlen: socklen_t,
        ) -> c_int;
        pub fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int;
        pub fn recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> ssize_t;
        pub fn recvfrom(
            fd: c_int,
            buf: *mut c_void,
            len: usize,
            flags: c_int,
            addr: *mut sockaddr,
            addrlen: *mut socklen_t,
        ) -> ssize_t;
        pub fn send(fd: c_int, buf: *const c_void, len: usize, flags: c_int) -> ssize_t;
        pub fn sendto(
            fd: c_int,
            buf: *const c_void,
            len: usize,
            flags: c_int,
            dest: *const sockaddr,
            addrlen: socklen_t,
        ) -> ssize_t;
        pub fn getaddrinfo(
            node: *const c_char,
            service: *const c_char,
            hints: *const addrinfo,
            res: *mut *mut addrinfo,
        ) -> c_int;
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
        // Multi-threaded platform — yield ~1 ms so the Zephyr scheduler
        // can run the read tasks. Matches the posix backend's approach.
        Self::sleep_ms(1);
        0
    }
}

// ============================================================================
// UDP multicast — stubbed
// ============================================================================
//
// Zephyr's zenoh-pico tests use `tcp/…` locators and `CONFIG_NROS_ZENOH_SCOUTING=n`
// in every example's prj.conf, so multicast open/listen/read/send are
// never exercised. Proper implementation is tracked in Phase 80 follow-ups.

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
