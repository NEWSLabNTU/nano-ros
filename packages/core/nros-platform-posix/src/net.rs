//! POSIX TCP/UDP networking via libc.
//!
//! Implements `PlatformTcp`, `PlatformUdp`, and `PlatformSocketHelpers`
//! for POSIX systems using BSD sockets.

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
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
        }
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
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
        }
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
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
        }
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
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
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
        // For multi-threaded POSIX, this uses select() on peer sockets.
        // The full implementation requires access to the peer list internals
        // which are zenoh-pico C types. For now, delegate to z_sleep_ms(1)
        // which yields the thread — same approach as the bare-metal poll.
        let _ = (peers, mutex);
        unsafe { libc::usleep(1000) }; // 1ms yield
        0
    }
}
