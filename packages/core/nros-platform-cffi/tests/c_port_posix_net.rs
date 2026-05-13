//! Phase 121.6.posix-c — runtime tests against the POSIX C net port.
//!
//! Drives a TCP loopback round-trip and a UDP loopback round-trip
//! through the canonical `nros_platform_*` symbols defined in
//! `../nros-platform-posix/src/net.c`. Multicast paths are
//! intentionally stubbed and not exercised here.

#![cfg(feature = "posix-c-port")]

use core::ffi::c_void;
use std::{mem::MaybeUninit, thread, time::Duration};

// Force the nros-platform-cffi rlib into the test binary so cargo
// honours its `cargo:rustc-link-lib=static=nros_platform_posix`
// directive. Without this the linker drops the static lib and the
// extern "C" references below stay unresolved.
#[allow(unused_imports)]
use nros_platform_cffi::CffiPlatform;

#[repr(C)]
struct PosixEndpoint {
    iptcp: *mut c_void,
}

#[repr(C)]
struct PosixSocket {
    fd: i32,
}

unsafe extern "C" {
    fn nros_platform_tcp_create_endpoint(ep: *mut c_void, addr: *const u8, port: *const u8) -> i8;
    fn nros_platform_tcp_free_endpoint(ep: *mut c_void);
    fn nros_platform_tcp_open(sock: *mut c_void, ep: *const c_void, timeout_ms: u32) -> i8;
    fn nros_platform_tcp_listen(sock: *mut c_void, ep: *const c_void) -> i8;
    fn nros_platform_tcp_close(sock: *mut c_void);
    fn nros_platform_tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize;
    fn nros_platform_socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8;

    fn nros_platform_udp_create_endpoint(ep: *mut c_void, addr: *const u8, port: *const u8) -> i8;
    fn nros_platform_udp_free_endpoint(ep: *mut c_void);
    fn nros_platform_udp_open(sock: *mut c_void, ep: *const c_void, timeout_ms: u32) -> i8;
    fn nros_platform_udp_listen(sock: *mut c_void, ep: *const c_void, timeout_ms: u32) -> i8;
    fn nros_platform_udp_close(sock: *mut c_void);
    fn nros_platform_udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        ep: *const c_void,
    ) -> usize;

    fn nros_platform_network_poll();
}

const ERR: usize = usize::MAX;

#[test]
fn tcp_loopback_roundtrip() {
    let addr = b"127.0.0.1\0";
    // Port 0 lets the OS assign — but our endpoint API takes a port
    // *string*; using ephemeral selection here would force a getsockname
    // dance. Use a fixed high port instead and accept the slight
    // collision risk between concurrent test runs.
    let port = b"56301\0";

    unsafe {
        let mut ep = MaybeUninit::<PosixEndpoint>::zeroed();
        assert_eq!(
            nros_platform_tcp_create_endpoint(
                ep.as_mut_ptr() as *mut c_void,
                addr.as_ptr(),
                port.as_ptr(),
            ),
            0,
        );

        let mut server = MaybeUninit::<PosixSocket>::zeroed();
        assert_eq!(
            nros_platform_tcp_listen(
                server.as_mut_ptr() as *mut c_void,
                ep.as_ptr() as *const c_void,
            ),
            0,
        );

        let server_addr = server.as_mut_ptr() as usize;
        let ep_addr = ep.as_ptr() as usize;
        let accepted_thread = thread::spawn(move || {
            let server = server_addr as *const c_void;
            let mut con = MaybeUninit::<PosixSocket>::zeroed();
            assert_eq!(
                nros_platform_socket_accept(server, con.as_mut_ptr() as *mut c_void),
                0,
            );
            let mut buf = [0u8; 5];
            let n =
                nros_platform_tcp_read(con.as_ptr() as *const c_void, buf.as_mut_ptr(), buf.len());
            assert_ne!(n, ERR);
            assert_eq!(&buf[..n], b"hello"[..n].as_ref());
            nros_platform_tcp_close(con.as_mut_ptr() as *mut c_void);
            (ep_addr, server_addr)
        });

        // Brief grace so the accept thread is parked in accept() before
        // we connect.
        thread::sleep(Duration::from_millis(20));

        let mut client = MaybeUninit::<PosixSocket>::zeroed();
        assert_eq!(
            nros_platform_tcp_open(
                client.as_mut_ptr() as *mut c_void,
                ep.as_ptr() as *const c_void,
                1000,
            ),
            0,
        );

        let payload = b"hello";
        let sent = nros_platform_tcp_send(
            client.as_ptr() as *const c_void,
            payload.as_ptr(),
            payload.len(),
        );
        assert_eq!(sent, payload.len());

        nros_platform_tcp_close(client.as_mut_ptr() as *mut c_void);

        let _ = accepted_thread.join();

        nros_platform_tcp_close(server.as_mut_ptr() as *mut c_void);
        nros_platform_tcp_free_endpoint(ep.as_mut_ptr() as *mut c_void);
    }
}

#[test]
fn udp_loopback_roundtrip() {
    let addr = b"127.0.0.1\0";
    let port = b"56302\0";

    unsafe {
        let mut ep = MaybeUninit::<PosixEndpoint>::zeroed();
        assert_eq!(
            nros_platform_udp_create_endpoint(
                ep.as_mut_ptr() as *mut c_void,
                addr.as_ptr(),
                port.as_ptr(),
            ),
            0,
        );

        let mut server = MaybeUninit::<PosixSocket>::zeroed();
        assert_eq!(
            nros_platform_udp_listen(
                server.as_mut_ptr() as *mut c_void,
                ep.as_ptr() as *const c_void,
                100,
            ),
            0,
        );

        let mut client = MaybeUninit::<PosixSocket>::zeroed();
        assert_eq!(
            nros_platform_udp_open(
                client.as_mut_ptr() as *mut c_void,
                ep.as_ptr() as *const c_void,
                100,
            ),
            0,
        );

        let payload = b"ping";
        let sent = nros_platform_udp_send(
            client.as_ptr() as *const c_void,
            payload.as_ptr(),
            payload.len(),
            ep.as_ptr() as *const c_void,
        );
        assert_eq!(sent, payload.len());

        let mut buf = [0u8; 4];
        let n = nros_platform_udp_read(
            server.as_ptr() as *const c_void,
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_ne!(n, ERR);
        assert_eq!(&buf[..n], &payload[..n]);

        nros_platform_udp_close(client.as_mut_ptr() as *mut c_void);
        nros_platform_udp_close(server.as_mut_ptr() as *mut c_void);
        nros_platform_udp_free_endpoint(ep.as_mut_ptr() as *mut c_void);
    }
}

#[test]
fn network_poll_is_noop() {
    // No assertion — just confirm the symbol exists + the call doesn't crash.
    unsafe { nros_platform_network_poll() };
}
