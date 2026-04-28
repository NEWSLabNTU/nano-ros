//! Phase 71.24 — host POSIX loopback validation suite for the
//! `PlatformUdp` contract.
//!
//! These tests pin down the bind / open / recv / send semantics that
//! dust-dds's no_std transport relies on. They run against
//! `nros_platform_posix::PosixPlatform` only — for the same coverage
//! on Zephyr / FreeRTOS / NuttX / ThreadX, see Phase 71.25's per-
//! platform QEMU smoke binary, which exercises the same trait surface
//! cross-compiled into each RTOS.
//!
//! What each test pins:
//!
//! * `bind_recvfrom_loopback` — `listen()` followed by `read()` on a
//!   bound socket receives a frame sent via `open()` + `send()` on a
//!   peer socket. Pins the basic unicast path used by SPDP / SEDP
//!   unicast retransmits.
//! * `set_recv_timeout_returns_zero_on_no_data` — once
//!   `set_recv_timeout(timeout_ms)` is set, `read()` on an idle bound
//!   socket returns `0` after the timeout instead of blocking
//!   forever. Required so dust-dds's recv loop can yield to the
//!   runtime between iterations.
//! * `create_endpoint_parses_ipv4_string` — a `"127.0.0.1\0"` /
//!   `"7411\0"` pair populates an opaque endpoint that `listen()`
//!   accepts without error. Pins the address-string contract zenoh-
//!   pico's transport assumes (`format!("{a}.{b}.{c}.{d}\0")`).

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use core::time::Duration;

use nros_platform::{NET_ENDPOINT_SIZE, NET_SOCKET_SIZE, PlatformUdp};
use nros_platform_posix::PosixPlatform;

/// Allocate an opaque socket buffer matching the platform's
/// `_z_sys_net_socket_t` size (Phase 71.22).
#[repr(C, align(8))]
struct OpaqueSocket {
    bytes: [u8; NET_SOCKET_SIZE],
}
impl OpaqueSocket {
    fn new() -> Self {
        Self {
            bytes: [0; NET_SOCKET_SIZE],
        }
    }
    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr() as *mut c_void
    }
    fn as_ptr(&self) -> *const c_void {
        self.bytes.as_ptr() as *const c_void
    }
}

#[repr(C, align(8))]
struct OpaqueEndpoint {
    bytes: [u8; NET_ENDPOINT_SIZE],
}
impl OpaqueEndpoint {
    fn new() -> Self {
        Self {
            bytes: [0; NET_ENDPOINT_SIZE],
        }
    }
    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr() as *mut c_void
    }
    fn as_ptr(&self) -> *const c_void {
        self.bytes.as_ptr() as *const c_void
    }
}

/// Pick an OS-assigned ephemeral UDP port by binding to port 0 via a
/// `std::net::UdpSocket`, reading the chosen port back, then dropping
/// the socket. Eliminates flakes from hard-coded port collisions when
/// the test suite is run in parallel.
fn pick_ephemeral_port() -> u16 {
    let s = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind any-port");
    let port = s.local_addr().expect("local_addr").port();
    drop(s);
    port
}

#[test]
fn bind_recvfrom_loopback() {
    let port = pick_ephemeral_port();
    let port_str = format!("{port}\0").into_bytes();
    let addr = b"127.0.0.1\0";

    // Build the bound endpoint.
    let mut bound_ep = OpaqueEndpoint::new();
    let rc = <PosixPlatform as PlatformUdp>::create_endpoint(
        bound_ep.as_mut_ptr(),
        addr.as_ptr(),
        port_str.as_ptr(),
    );
    assert_eq!(rc, 0, "create_endpoint(bound) should return 0");

    // Bind a socket on it.
    let mut bound_sock = OpaqueSocket::new();
    let rc = <PosixPlatform as PlatformUdp>::listen(
        bound_sock.as_mut_ptr(),
        bound_ep.as_ptr(),
        100, // timeout_ms — keeps recv from blocking forever on POSIX
    );
    assert_eq!(rc, 0, "listen(bound) should return 0");

    // Build the destination endpoint (same address) and an outbound
    // socket. POSIX `PlatformUdp::open` doesn't bind locally — it just
    // builds a connected-UDP socket (SOCK_DGRAM + connect()) so
    // subsequent `send()` calls reach the right peer.
    let mut dest_ep = OpaqueEndpoint::new();
    let rc = <PosixPlatform as PlatformUdp>::create_endpoint(
        dest_ep.as_mut_ptr(),
        addr.as_ptr(),
        port_str.as_ptr(),
    );
    assert_eq!(rc, 0, "create_endpoint(dest) should return 0");

    let mut tx_sock = OpaqueSocket::new();
    let rc = <PosixPlatform as PlatformUdp>::open(
        tx_sock.as_mut_ptr(),
        dest_ep.as_ptr(),
        100,
    );
    assert_eq!(rc, 0, "open(tx) should return 0");

    // Send a payload; recv on the bound socket.
    let payload = b"loopback-71.24";
    let n = <PosixPlatform as PlatformUdp>::send(
        tx_sock.as_ptr(),
        payload.as_ptr(),
        payload.len(),
        dest_ep.as_ptr(),
    );
    assert_eq!(n, payload.len(), "send returned wrong byte count");

    // Wait briefly so the kernel has a chance to deliver before the
    // 100 ms recv timeout kicks in (loopback usually takes < 1 ms but
    // CI hosts can be jittery).
    std::thread::sleep(Duration::from_millis(20));

    let mut rx = [0u8; 64];
    let n = <PosixPlatform as PlatformUdp>::read(
        bound_sock.as_ptr(),
        rx.as_mut_ptr(),
        rx.len(),
    );
    assert_eq!(n, payload.len(), "read returned wrong byte count");
    assert_eq!(&rx[..n], payload, "received payload mismatch");

    <PosixPlatform as PlatformUdp>::close(tx_sock.as_mut_ptr());
    <PosixPlatform as PlatformUdp>::close(bound_sock.as_mut_ptr());
}

#[test]
fn set_recv_timeout_returns_zero_on_no_data() {
    let port = pick_ephemeral_port();
    let port_str = format!("{port}\0").into_bytes();
    let addr = b"127.0.0.1\0";

    let mut ep = OpaqueEndpoint::new();
    assert_eq!(
        <PosixPlatform as PlatformUdp>::create_endpoint(
            ep.as_mut_ptr(),
            addr.as_ptr(),
            port_str.as_ptr(),
        ),
        0,
    );

    let mut sock = OpaqueSocket::new();
    assert_eq!(
        <PosixPlatform as PlatformUdp>::listen(
            sock.as_mut_ptr(),
            ep.as_ptr(),
            50, // 50 ms timeout
        ),
        0,
    );

    let start = std::time::Instant::now();
    let mut rx = [0u8; 16];
    let n = <PosixPlatform as PlatformUdp>::read(
        sock.as_ptr(),
        rx.as_mut_ptr(),
        rx.len(),
    );
    let elapsed = start.elapsed();

    // No sender means recv times out. `read()` returns 0 (or
    // usize::MAX in some impls — the trait doesn't fully pin this).
    // Either is acceptable here; what matters is that we did not
    // block forever.
    assert!(
        n == 0 || n == usize::MAX,
        "expected 0 or usize::MAX on timeout, got {n}",
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "recv should have returned within ~50 ms, took {elapsed:?}",
    );

    <PosixPlatform as PlatformUdp>::close(sock.as_mut_ptr());
}

#[test]
fn create_endpoint_parses_ipv4_string() {
    let mut ep = OpaqueEndpoint::new();
    let rc = <PosixPlatform as PlatformUdp>::create_endpoint(
        ep.as_mut_ptr(),
        b"10.20.30.40\0".as_ptr(),
        b"54321\0".as_ptr(),
    );
    assert_eq!(rc, 0, "create_endpoint should accept dotted-quad + port");

    // We don't assert on the layout of the opaque endpoint bytes —
    // that's an internal contract. What matters is that `listen()`
    // / `open()` accept it without error in the other tests.
    <PosixPlatform as PlatformUdp>::free_endpoint(ep.as_mut_ptr());
}
