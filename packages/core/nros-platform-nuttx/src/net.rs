//! NuttX POSIX socket networking — TODO: implement using nuttx-sys types.
//!
//! Currently networking goes through C unix/network.c.
//! This module will replace it using nuttx_sys bindgen types.
//! For now, delegate to PosixPlatform networking.

use crate::NuttxPlatform;
use core::ffi::c_void;

// Delegate all networking to PosixPlatform for now.
// TODO(phase-80.10): Replace with nuttx_sys types when activated.
impl NuttxPlatform {
    pub fn tcp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        nros_platform_posix::PosixPlatform::tcp_create_endpoint(ep, address, port)
    }
    pub fn tcp_free_endpoint(ep: *mut c_void) {
        nros_platform_posix::PosixPlatform::tcp_free_endpoint(ep)
    }
    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        nros_platform_posix::PosixPlatform::tcp_open(sock, endpoint, timeout_ms)
    }
    pub fn tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        nros_platform_posix::PosixPlatform::tcp_listen(sock, endpoint)
    }
    pub fn tcp_close(sock: *mut c_void) {
        nros_platform_posix::PosixPlatform::tcp_close(sock)
    }
    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        nros_platform_posix::PosixPlatform::tcp_read(sock, buf, len)
    }
    pub fn tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        nros_platform_posix::PosixPlatform::tcp_read_exact(sock, buf, len)
    }
    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        nros_platform_posix::PosixPlatform::tcp_send(sock, buf, len)
    }
    pub fn udp_create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        nros_platform_posix::PosixPlatform::udp_create_endpoint(ep, address, port)
    }
    pub fn udp_free_endpoint(ep: *mut c_void) {
        nros_platform_posix::PosixPlatform::udp_free_endpoint(ep)
    }
    pub fn udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        nros_platform_posix::PosixPlatform::udp_open(sock, endpoint, timeout_ms)
    }
    pub fn udp_close(sock: *mut c_void) {
        nros_platform_posix::PosixPlatform::udp_close(sock)
    }
    pub fn udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        nros_platform_posix::PosixPlatform::udp_read(sock, buf, len)
    }
    pub fn udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        nros_platform_posix::PosixPlatform::udp_read_exact(sock, buf, len)
    }
    pub fn udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        nros_platform_posix::PosixPlatform::udp_send(sock, buf, len, endpoint)
    }
    pub fn socket_set_non_blocking(sock: *const c_void) -> i8 {
        nros_platform_posix::PosixPlatform::socket_set_non_blocking(sock)
    }
    pub fn socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        nros_platform_posix::PosixPlatform::socket_accept(sock_in, sock_out)
    }
    pub fn socket_close(sock: *mut c_void) {
        nros_platform_posix::PosixPlatform::socket_close(sock)
    }
    pub fn socket_wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8 {
        nros_platform_posix::PosixPlatform::socket_wait_event(peers, mutex)
    }
    pub fn mcast_open(
        _s: *mut c_void,
        _e: *const c_void,
        _l: *mut c_void,
        _t: u32,
        _i: *const u8,
    ) -> i8 {
        -1
    }
    pub fn mcast_listen(
        _s: *mut c_void,
        _e: *const c_void,
        _t: u32,
        _i: *const u8,
        _j: *const u8,
    ) -> i8 {
        -1
    }
    pub fn mcast_close(_r: *mut c_void, _s: *mut c_void, _re: *const c_void, _le: *const c_void) {}
    pub fn mcast_read(
        _s: *const c_void,
        _b: *mut u8,
        _l: usize,
        _le: *const c_void,
        _a: *mut c_void,
    ) -> usize {
        usize::MAX
    }
    pub fn mcast_read_exact(
        _s: *const c_void,
        _b: *mut u8,
        _l: usize,
        _le: *const c_void,
        _a: *mut c_void,
    ) -> usize {
        usize::MAX
    }
    pub fn mcast_send(_s: *const c_void, _b: *const u8, _l: usize, _e: *const c_void) -> usize {
        usize::MAX
    }
}
