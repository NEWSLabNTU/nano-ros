//! `define_smoltcp_platform!` macro — emit the per-platform smoltcp
//! networking method blocks once instead of copy-pasting them across
//! every bare-metal `nros-platform-*` crate.
//!
//! The 4 bare-metal platform crates (MPS2-AN385, STM32F4, ESP32,
//! ESP32-QEMU) used to each carry an identical 502-line `net.rs` whose
//! only per-platform difference was the `impl <PlatformType>` line.
//! This macro takes the platform's ZST identifier and expands to the
//! same five `impl` blocks (TCP, UDP, socket helpers, multicast stubs)
//! that the platform shim then dispatches to via inherent-method calls.

/// Emit `PlatformTcp`, `PlatformUdp`, `PlatformSocketHelpers`, and
/// `PlatformUdpMulticast` trait impls on the given platform ZST,
/// backed by a `SmoltcpBridge` on the caller side.
///
/// Usage:
/// ```ignore
/// pub struct Mps2An385Platform;
/// nros_smoltcp::define_smoltcp_platform!(Mps2An385Platform);
/// ```
///
/// All four trait impls are emitted by a single invocation. Shims
/// dispatch via the usual qualified path
/// (`<ConcretePlatform as PlatformTcp>::open(...)`).
///
/// Phase 84.F4.4 changed this macro from inherent-method output to
/// trait-impl output; method names lost their `tcp_` / `udp_` /
/// `socket_` prefixes because the trait name already namespaces them.
/// `PlatformUdpMulticast` kept the `mcast_*` prefix because the trait
/// and the Tcp/Udp `open` / `read` etc. would otherwise collide at
/// the call site and hurt readability.
#[macro_export]
macro_rules! define_smoltcp_platform {
    ($plat:ident) => {
        $crate::__define_smoltcp_platform_impl!($plat);
    };
}

/// Internal implementation of [`define_smoltcp_platform!`]. Wrapped in
/// a private module so we can put `#![allow(...)]` inner attributes on
/// the module item itself rather than on a const-block expression
/// (which Rust rejects as `attributes on expressions are experimental`).
#[doc(hidden)]
#[macro_export]
macro_rules! __define_smoltcp_platform_impl {
    ($plat:ident) => {
        #[doc(hidden)]
        #[allow(unsafe_op_in_unsafe_fn)]
        mod __nros_smoltcp_platform_impl {
            use ::core::ffi::c_void;

            use $crate::SmoltcpBridge;
            use $crate::{CONNECT_TIMEOUT_MS, SOCKET_TIMEOUT_MS};

            /// Per-call UDP receive timeout, updated by `udp_set_recv_timeout`.
            static mut UDP_RECV_TIMEOUT_MS: u64 = SOCKET_TIMEOUT_MS;

            // ---- C struct layouts (must match bare-metal/platform.h) ----

            /// Socket: `{ int8_t _handle; bool _connected; }`
            #[repr(C)]
            struct Socket {
                _handle: i8,
                _connected: bool,
            }

            /// Endpoint: `{ uint8_t _ip[4]; uint16_t _port; }`
            #[repr(C)]
            struct Endpoint {
                _ip: [u8; 4],
                _port: u16,
            }

            // ---- IP / port parsing ----

            fn parse_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
                let ep = ep as *mut Endpoint;

                let ip = match unsafe { $crate::util::parse_ip_address(address) } {
                    Some(ip) => ip,
                    None => return -1,
                };

                let p = match unsafe { $crate::util::parse_port(port) } {
                    Some(p) => p,
                    None => return -1,
                };

                unsafe {
                    (*ep)._ip = ip;
                    (*ep)._port = p;
                }
                0
            }

            // ---- TCP ----

            impl $crate::PlatformTcp for crate::$plat {
                fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
                    if ep.is_null() || address.is_null() || port.is_null() {
                        return -1;
                    }
                    parse_endpoint(ep, address, port)
                }

                fn free_endpoint(_ep: *mut c_void) {}

                fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
                    if sock.is_null() || endpoint.is_null() {
                        return -1;
                    }

                    let sock = sock as *mut Socket;
                    let rep = unsafe { &*(endpoint as *const Endpoint) };

                    unsafe {
                        (*sock)._handle = -1;
                        (*sock)._connected = false;
                    }

                    let handle = SmoltcpBridge::tcp_open();
                    if handle < 0 {
                        return -1;
                    }

                    unsafe {
                        (*sock)._handle = handle as i8;
                    }

                    if SmoltcpBridge::tcp_connect(handle, &rep._ip, rep._port) < 0 {
                        SmoltcpBridge::tcp_close(handle);
                        unsafe {
                            (*sock)._handle = -1;
                        }
                        return -1;
                    }

                    let timeout = if timeout_ms > 0 {
                        timeout_ms as u64
                    } else {
                        CONNECT_TIMEOUT_MS
                    };
                    let start = SmoltcpBridge::clock_now_ms();

                    loop {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::tcp_is_connected(handle) {
                            unsafe {
                                (*sock)._connected = true;
                            }
                            return 0;
                        }

                        if SmoltcpBridge::clock_now_ms() - start > timeout {
                            SmoltcpBridge::tcp_close(handle);
                            unsafe {
                                (*sock)._handle = -1;
                            }
                            return -1;
                        }
                    }
                }

                fn listen(_sock: *mut c_void, _endpoint: *const c_void) -> i8 {
                    -1
                }

                fn close(sock: *mut c_void) {
                    if sock.is_null() {
                        return;
                    }
                    let sock = sock as *mut Socket;
                    unsafe {
                        let handle = (*sock)._handle;
                        if handle >= 0 {
                            SmoltcpBridge::tcp_close(handle as i32);
                            (*sock)._handle = -1;
                            (*sock)._connected = false;
                        }
                    }
                }

                fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() || len == 0 {
                        return usize::MAX;
                    }

                    let handle = sock._handle as i32;

                    SmoltcpBridge::poll_network();

                    if SmoltcpBridge::tcp_can_recv(handle) {
                        let slice = unsafe { ::core::slice::from_raw_parts_mut(buf, len) };
                        let received = SmoltcpBridge::tcp_recv(handle, slice);
                        if received > 0 {
                            return received as usize;
                        }
                    }

                    if !SmoltcpBridge::tcp_is_connected(handle) {
                        return usize::MAX;
                    }

                    0
                }

                fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() {
                        return usize::MAX;
                    }
                    if len == 0 {
                        return 0;
                    }

                    let handle = sock._handle as i32;
                    let mut total: usize = 0;
                    let mut start = SmoltcpBridge::clock_now_ms();

                    while total < len {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::tcp_can_recv(handle) {
                            let remaining = len - total;
                            let slice = unsafe {
                                ::core::slice::from_raw_parts_mut(buf.add(total), remaining)
                            };
                            let received = SmoltcpBridge::tcp_recv(handle, slice);
                            if received > 0 {
                                total += received as usize;
                                start = SmoltcpBridge::clock_now_ms();
                            }
                        }

                        if !SmoltcpBridge::tcp_is_connected(handle) {
                            return usize::MAX;
                        }

                        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                            return usize::MAX;
                        }
                    }

                    total
                }

                fn send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() {
                        return usize::MAX;
                    }
                    if len == 0 {
                        return 0;
                    }

                    let handle = sock._handle as i32;
                    let mut total: usize = 0;
                    let mut start = SmoltcpBridge::clock_now_ms();

                    while total < len {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::tcp_can_send(handle) {
                            let remaining = len - total;
                            let data =
                                unsafe { ::core::slice::from_raw_parts(buf.add(total), remaining) };
                            let sent = SmoltcpBridge::tcp_send(handle, data);
                            if sent > 0 {
                                total += sent as usize;
                                start = SmoltcpBridge::clock_now_ms();
                            }
                        }

                        if !SmoltcpBridge::tcp_is_connected(handle) {
                            return usize::MAX;
                        }

                        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                            return usize::MAX;
                        }
                    }

                    SmoltcpBridge::poll_network();
                    total
                }
            }

            // ---- UDP ----

            impl $crate::PlatformUdp for crate::$plat {
                fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
                    if ep.is_null() || address.is_null() || port.is_null() {
                        return -1;
                    }
                    parse_endpoint(ep, address, port)
                }

                fn free_endpoint(_ep: *mut c_void) {}

                fn open(sock: *mut c_void, endpoint: *const c_void, _timeout_ms: u32) -> i8 {
                    if sock.is_null() || endpoint.is_null() {
                        return -1;
                    }

                    let sock = sock as *mut Socket;
                    let rep = unsafe { &*(endpoint as *const Endpoint) };

                    unsafe {
                        (*sock)._handle = -1;
                        (*sock)._connected = false;
                    }

                    let handle = SmoltcpBridge::udp_open();
                    if handle < 0 {
                        return -1;
                    }

                    if SmoltcpBridge::udp_set_remote(handle, &rep._ip, rep._port) < 0 {
                        SmoltcpBridge::udp_close(handle);
                        return -1;
                    }

                    unsafe {
                        (*sock)._handle = handle as i8;
                        (*sock)._connected = true;
                    }

                    0
                }

                fn close(sock: *mut c_void) {
                    if sock.is_null() {
                        return;
                    }
                    let sock = sock as *mut Socket;
                    unsafe {
                        let handle = (*sock)._handle;
                        if handle >= 0 {
                            SmoltcpBridge::udp_close(handle as i32);
                            (*sock)._handle = -1;
                            (*sock)._connected = false;
                        }
                    }
                }

                fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() || len == 0 {
                        return usize::MAX;
                    }

                    let handle = sock._handle as i32;
                    let timeout = unsafe { UDP_RECV_TIMEOUT_MS };
                    let start = SmoltcpBridge::clock_now_ms();

                    loop {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::udp_can_recv(handle) {
                            let slice = unsafe { ::core::slice::from_raw_parts_mut(buf, len) };
                            let received = SmoltcpBridge::udp_recv(handle, slice);
                            if received > 0 {
                                return received as usize;
                            }
                        }

                        if SmoltcpBridge::clock_now_ms() - start > timeout {
                            return usize::MAX;
                        }
                    }
                }

                fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() {
                        return usize::MAX;
                    }
                    if len == 0 {
                        return 0;
                    }

                    let handle = sock._handle as i32;
                    let mut total: usize = 0;
                    let mut start = SmoltcpBridge::clock_now_ms();

                    while total < len {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::udp_can_recv(handle) {
                            let remaining = len - total;
                            let slice = unsafe {
                                ::core::slice::from_raw_parts_mut(buf.add(total), remaining)
                            };
                            let received = SmoltcpBridge::udp_recv(handle, slice);
                            if received > 0 {
                                total += received as usize;
                                start = SmoltcpBridge::clock_now_ms();
                            }
                        }

                        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                            return usize::MAX;
                        }
                    }

                    total
                }

                fn send(
                    sock: *const c_void,
                    buf: *const u8,
                    len: usize,
                    endpoint: *const c_void,
                ) -> usize {
                    let sock = unsafe { &*(sock as *const Socket) };
                    if sock._handle < 0 || buf.is_null() {
                        return usize::MAX;
                    }
                    if len == 0 {
                        return 0;
                    }

                    let rep = unsafe { &*(endpoint as *const Endpoint) };
                    let handle = sock._handle as i32;
                    let mut total: usize = 0;
                    let mut start = SmoltcpBridge::clock_now_ms();

                    while total < len {
                        SmoltcpBridge::poll_network();

                        if SmoltcpBridge::udp_can_send(handle) {
                            let remaining = len - total;
                            let data =
                                unsafe { ::core::slice::from_raw_parts(buf.add(total), remaining) };
                            let sent = SmoltcpBridge::udp_send(handle, data, &rep._ip, rep._port);
                            if sent > 0 {
                                total += sent as usize;
                                start = SmoltcpBridge::clock_now_ms();
                            }
                        }

                        if SmoltcpBridge::clock_now_ms() - start > SOCKET_TIMEOUT_MS {
                            return usize::MAX;
                        }
                    }

                    total
                }

                fn set_recv_timeout(_sock: *const c_void, timeout_ms: u32) {
                    unsafe {
                        UDP_RECV_TIMEOUT_MS = if timeout_ms == 0 {
                            SOCKET_TIMEOUT_MS
                        } else {
                            timeout_ms as u64
                        };
                    }
                }

                /// Phase 71.21 — bind a UDP socket to the endpoint's
                /// local port so the smoltcp UDP socket listens on a
                /// known port (required by DDS RTPS PSM §9.6.1.4).
                ///
                /// The smoltcp `UdpSocket::bind()` happens lazily in
                /// the bridge's poll loop; here we just record
                /// `entry.local_port` via `udp_set_local_port`. Until
                /// the next poll the socket isn't yet bound, but the
                /// recv loop in `nros-rmw-dds::transport_nros` yields
                /// repeatedly anyway so the bind happens before the
                /// first read attempt.
                fn listen(sock: *mut c_void, endpoint: *const c_void, _timeout_ms: u32) -> i8 {
                    if sock.is_null() || endpoint.is_null() {
                        return -1;
                    }
                    let sock = sock as *mut Socket;
                    let rep = unsafe { &*(endpoint as *const Endpoint) };

                    unsafe {
                        (*sock)._handle = -1;
                        (*sock)._connected = false;
                    }

                    let handle = SmoltcpBridge::udp_open();
                    if handle < 0 {
                        return -1;
                    }

                    if SmoltcpBridge::udp_set_local_port(handle, rep._port) < 0 {
                        SmoltcpBridge::udp_close(handle);
                        return -1;
                    }

                    unsafe {
                        (*sock)._handle = handle as i8;
                        (*sock)._connected = false;
                    }
                    0
                }
            }

            // ---- Socket helpers ----

            impl $crate::PlatformSocketHelpers for crate::$plat {
                fn set_non_blocking(_sock: *const c_void) -> i8 {
                    0
                }

                fn accept(_sock_in: *const c_void, _sock_out: *mut c_void) -> i8 {
                    -1
                }

                fn close(sock: *mut c_void) {
                    // Reuse the TCP close path (both types carry Socket bytes).
                    <crate::$plat as $crate::PlatformTcp>::close(sock);
                }

                fn wait_event(_peers: *mut c_void, _mutex: *mut c_void) -> i8 {
                    SmoltcpBridge::poll_network();
                    0
                }
            }

            // ---- UDP multicast stubs (not supported on bare-metal) ----

            impl $crate::PlatformUdpMulticast for crate::$plat {
                fn mcast_open(
                    _sock: *mut c_void,
                    _endpoint: *const c_void,
                    _lep: *mut c_void,
                    _timeout_ms: u32,
                    _iface: *const u8,
                ) -> i8 {
                    -1
                }

                fn mcast_listen(
                    _sock: *mut c_void,
                    _endpoint: *const c_void,
                    _timeout_ms: u32,
                    _iface: *const u8,
                    _join: *const u8,
                ) -> i8 {
                    -1
                }

                fn mcast_close(
                    _sockrecv: *mut c_void,
                    _socksend: *mut c_void,
                    _rep: *const c_void,
                    _lep: *const c_void,
                ) {
                }

                fn mcast_read(
                    _sock: *const c_void,
                    _buf: *mut u8,
                    _len: usize,
                    _lep: *const c_void,
                    _addr: *mut c_void,
                ) -> usize {
                    usize::MAX
                }

                fn mcast_read_exact(
                    _sock: *const c_void,
                    _buf: *mut u8,
                    _len: usize,
                    _lep: *const c_void,
                    _addr: *mut c_void,
                ) -> usize {
                    usize::MAX
                }

                fn mcast_send(
                    _sock: *const c_void,
                    _buf: *const u8,
                    _len: usize,
                    _endpoint: *const c_void,
                ) -> usize {
                    usize::MAX
                }
            }
        }
    };
}
