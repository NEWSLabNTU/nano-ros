# Phase 80: Unified Network Interface for nros-platform

**Goal**: Extend the nros-platform abstraction to cover networking (TCP/UDP socket operations), making the RMW transport layer fully platform-agnostic.

**Status**: In Progress (80.1–80.10, 80.12 done)
**Priority**: Medium
**Depends on**: Phase 79 (Unified Platform Abstraction Layer)

## Overview

### Problem

zenoh-pico's networking is implemented per-platform in C `network.c` files:

- `unix/network.c` — POSIX BSD sockets (~1050 lines)
- `freertos/lwip/network.c` — lwIP sockets (~730 lines)
- `freertos/freertos_plus_tcp/network.c` — FreeRTOS-Plus-TCP (~340 lines)
- `zpico-smoltcp` Rust crate — bare-metal smoltcp (~600 lines)
- `threadx/network.c` — NetX Duo BSD sockets (~500 lines)

Each implements the same set of ~24 `extern "C"` functions (`_z_open_tcp`,
`_z_read_tcp`, `_z_send_tcp`, etc.) but with platform-specific socket APIs.
This C code is compiled inside `zpico-sys`, coupling the RMW layer to
specific networking stacks.

### Solution

Add networking traits to `nros-platform` with opaque socket/endpoint types.
The shim in `zpico-sys` forwards zenoh-pico's `_z_open_tcp` etc. to
`ConcretePlatform::tcp_open()`, same as it already does for `z_clock_now`
→ `ConcretePlatform::clock_ms()`.

```
zenoh-pico C code
  calls _z_open_tcp(sock, endpoint, timeout)
       ↓
zpico-platform-shim (inside zpico-sys)
  forwards to ConcretePlatform::tcp_open(sock, endpoint, timeout)
       ↓
nros-platform-<impl>
  POSIX:    libc::socket(), libc::connect()
  lwIP:     lwip_socket(), lwip_connect() (via cffi vtable from C)
  smoltcp:  zpico-smoltcp bridge (Rust, bare-metal)
  NetX:     nx_tcp_socket_create() (via cffi vtable from C)
```

### Benefits

- Board crates become fully transport-agnostic (no more C `network.c` in zpico-sys)
- Network implementations can be Rust (POSIX, smoltcp) or C (lwIP, NetX via cffi)
- Adding a new networking stack requires only an nros-platform implementation
- Enables future network stack sharing across RMW backends (zenoh + XRCE over same sockets)

## Per-platform architecture

### Current (before Phase 80)

POSIX and FreeRTOS have networking **inside** zpico-sys (C `network.c`).
Bare-metal has it **outside** (zpico-smoltcp crate + board crate). This is
inconsistent — the RMW layer is coupled to platform-specific networking code.

**POSIX:**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  ├── unix/network.c (BSD sockets)              ← C networking inside zpico-sys
  └── zpico-platform-shim → nros-platform-posix (clock, malloc, threading)
```

**FreeRTOS:**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  ├── freertos/lwip/network.c (lwIP sockets)    ← C networking inside zpico-sys
  └── zpico-platform-shim → nros-platform-freertos (clock, malloc, threading)
Board crate:
  └── nros-mps2-an385-freertos (lwIP init, LAN9118 driver)
```

**Bare-metal (MPS2-AN385):**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  └── zpico-platform-shim → nros-platform-mps2-an385 (clock, malloc, socket stubs)
Board crate:
  ├── nros-mps2-an385 (LAN9118 driver, smoltcp Interface)
  ├── zpico-smoltcp (_z_open_tcp via smoltcp)   ← Rust networking outside zpico-sys
  └── network.rs (smoltcp_network_poll)
```

### Proposed (after Phase 80)

All networking moves to `nros-platform`. zpico-sys has zero network code.
The board crate is the single integration point for hardware + networking.

**POSIX:**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  └── zpico-platform-shim → nros-platform-posix
        ├── PlatformClock, PlatformAlloc, PlatformThreading (existing)
        └── PlatformTcp, PlatformUdp (NEW — libc::socket, libc::connect)
```

**FreeRTOS:**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  └── zpico-platform-shim → nros-platform-freertos
        ├── PlatformClock, PlatformAlloc, PlatformThreading (existing)
        └── PlatformTcp, PlatformUdp (NEW — via cffi vtable)
Board crate (nros-mps2-an385-freertos):
  ├── Hardware init (LAN9118, lwIP)
  ├── Registers cffi network vtable:
  │     tcp_open → lwip_socket + lwip_connect
  │     tcp_read → lwip_recv
  │     socket_wait_event → lwip_select
  └── C code: lwip_network_vtable.c (thin wrappers calling lwIP API)
```

**Bare-metal (MPS2-AN385):**
```
nros-rmw-zenoh → zpico-sys
  ├── zenoh-pico C code (calls _z_open_tcp etc.)
  └── zpico-platform-shim → nros-platform-mps2-an385
        ├── PlatformClock, PlatformAlloc, PlatformThreading (existing)
        └── PlatformTcp, PlatformUdp (NEW — smoltcp bridge, moved from zpico-smoltcp)
Board crate (nros-mps2-an385):
  ├── Hardware init (LAN9118, smoltcp Interface)
  └── smoltcp_network_poll (existing)
```

### Key change

```
Before:  zpico-sys compiles C network.c → platform networking inside RMW crate
After:   zpico-platform-shim forwards → nros-platform handles networking
```

The board crate becomes the single integration point for both system
primitives AND networking. RTOS boards register a C vtable with lwIP/NetX
wrappers. Bare-metal boards implement PlatformTcp/PlatformUdp directly in
Rust via smoltcp. POSIX implements via `libc`. All transparent to the RMW
layer.

## Detailed design

### Trait design

The interface uses opaque `*mut c_void` pointers for socket and endpoint
handles, same pattern as the threading interface (`_z_mutex_t` etc.):

```rust
// nros-platform/src/traits.rs

/// TCP networking.
pub trait PlatformTcp {
    /// Resolve address + port into an endpoint handle.
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8;
    fn free_endpoint(ep: *mut c_void);

    /// Open a TCP client connection with timeout.
    fn open(sock: *mut c_void, endpoint: c_void, timeout_ms: u32) -> i8;
    /// Open a TCP listening socket.
    fn listen(sock: *mut c_void, endpoint: c_void) -> i8;
    /// Close a TCP socket.
    fn close(sock: *mut c_void);

    /// Read up to `len` bytes. Returns bytes read, or SIZE_MAX on error.
    fn read(sock: c_void, buf: *mut u8, len: usize) -> usize;
    /// Read exactly `len` bytes. Returns bytes read, or SIZE_MAX on error.
    fn read_exact(sock: c_void, buf: *mut u8, len: usize) -> usize;
    /// Send `len` bytes. Returns bytes sent, or SIZE_MAX on error.
    fn send(sock: c_void, buf: *const u8, len: usize) -> usize;
}

/// UDP unicast networking.
pub trait PlatformUdp {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8;
    fn free_endpoint(ep: *mut c_void);
    fn open(sock: *mut c_void, endpoint: c_void, timeout_ms: u32) -> i8;
    fn close(sock: *mut c_void);
    fn read(sock: c_void, buf: *mut u8, len: usize) -> usize;
    fn send(sock: c_void, buf: *const u8, len: usize, endpoint: c_void) -> usize;
}

/// Socket helpers.
pub trait PlatformSocketHelpers {
    fn set_non_blocking(sock: *const c_void) -> i8;
    fn accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8;
    fn close(sock: *mut c_void);
    fn wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8;
}
```

### Pass-by-value challenge

zenoh-pico passes `_z_sys_net_socket_t` **by value** (not by pointer) in
read/send functions:

```c
size_t _z_read_tcp(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len);
size_t _z_send_tcp(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len);
```

The socket struct size varies per platform (4-16 bytes). The shim defines
opaque `#[repr(C)]` wrapper types whose sizes are **automatically derived
from the C headers** at build time — no manual size constants.

### Automatic type size detection (alloc-free)

The struct sizes are computed by the compiler, not hardcoded:

```
zpico-sys build.rs:
  1. Compile a C "size probe" file that encodes sizes in symbol arrays:
       #include "zenoh-pico/system/platform.h"
       const unsigned char __socket_size[sizeof(_z_sys_net_socket_t)] = {0};
       const unsigned char __endpoint_size[sizeof(_z_sys_net_endpoint_t)] = {0};
  2. Read symbol sizes from the .o file (llvm-nm or objdump)
  3. Emit DEP variables via cargo:SOCKET_SIZE=<N>, cargo:ENDPOINT_SIZE=<N>

zpico-platform-shim build.rs:
  1. Read DEP_ZPICO_SOCKET_SIZE and DEP_ZPICO_ENDPOINT_SIZE
  2. Generate platform_sizes.rs with const values

zpico-platform-shim/src/shim.rs:
  include!(concat!(env!("OUT_DIR"), "/platform_sizes.rs"));

  #[repr(C, align(8))]
  pub struct ZSysNetSocket { _opaque: [u8; SOCKET_SIZE] }
  #[repr(C, align(4))]
  pub struct ZSysNetEndpoint { _opaque: [u8; ENDPOINT_SIZE] }
```

This works for **any cross-compilation target** because it uses the same
cross-compiler that builds zenoh-pico — it compiles but never runs the probe.
Changing the C struct layout automatically updates the Rust types on next build.
No heap allocation needed — sockets are passed by value on the stack.

### Endpoint type sizes

| Platform             | Socket size | Endpoint size | Notes                                                              |
|----------------------|-------------|---------------|--------------------------------------------------------------------|
| POSIX                | 16 bytes    | 8 bytes       | `int fd` + `void* tls_sock` / `addrinfo*`                          |
| lwIP                 | 4 bytes     | 8 bytes       | `int socket` / `addrinfo*`                                         |
| FreeRTOS-Plus-TCP    | 4 bytes     | 8 bytes       | `Socket_t` / `freertos_addrinfo*`                                  |
| Bare-metal (smoltcp) | 16 bytes    | 6 bytes       | `{i8 handle, bool connected, void* tls}` / `{[u8;4] ip, u16 port}` |
| ThreadX (NetX)       | 8 bytes     | 8 bytes       | `void*` / `{u32 addr, u16 port}`                                   |

### C FFI vtable extension

The `nros-platform-cffi` vtable gets new function pointers for networking:

```c
typedef struct {
    // ... existing clock, alloc, sleep, random, time, threading fields ...

    // TCP
    int8_t (*tcp_create_endpoint)(void *ep, const uint8_t *addr, const uint8_t *port);
    void   (*tcp_free_endpoint)(void *ep);
    int8_t (*tcp_open)(void *sock, /* by-value endpoint */, uint32_t timeout);
    int8_t (*tcp_listen)(void *sock, /* by-value endpoint */);
    void   (*tcp_close)(void *sock);
    size_t (*tcp_read)(/* by-value sock */, uint8_t *buf, size_t len);
    size_t (*tcp_send)(/* by-value sock */, const uint8_t *buf, size_t len);

    // UDP
    // ... similar pattern ...

    // Socket helpers
    int8_t (*socket_set_non_blocking)(const void *sock);
    int8_t (*socket_wait_event)(void *peers, void *mutex);
} nros_platform_vtable_t;
```

## Work Items

- [x] 80.1 — Design opaque socket/endpoint types in nros-platform traits
  - [x] 80.1.1 — Define `PlatformTcp`, `PlatformUdp`, `PlatformSocketHelpers` traits
  - [x] 80.1.2 — Size probe in zpico-platform-shim build.rs (compiles C, reads .o symbol sizes)
  - [x] 80.1.3 — Generated `platform_net_sizes.rs` with auto-detected sizes per target
  - [x] 80.1.4 — `ZSysNetSocket` / `ZSysNetEndpoint` opaque wrappers with correct sizes
  - [x] 80.1.5 — Verified: POSIX x86_64=16/8, FreeRTOS ARM32=4/4, bare-metal ARM32=16/8
- [x] 80.2 — Add network functions to zpico-platform-shim
  - [x] 80.2.1 — TCP forwarders (8 functions): endpoint create/free, open, listen, close, read, read_exact, send
  - [x] 80.2.2 — UDP unicast forwarders (8 functions): endpoint create/free, open, listen, close, read, read_exact, send
  - [x] 80.2.3 — Socket helper forwarders: set_non_blocking, accept, close, wait_event
  - [x] 80.2.4 — Remove existing `socket_stubs` module (done in 80.6.7)
- [x] 80.3 — Implement `PlatformTcp`/`PlatformUdp` for POSIX
  - [x] 80.3.1 — TCP: getaddrinfo, socket, connect, recv, send, shutdown+close via libc
  - [x] 80.3.2 — UDP: getaddrinfo, socket, recvfrom, sendto via libc
  - [x] 80.3.3 — Socket helpers: fcntl(O_NONBLOCK), accept, socket_close, wait_event
  - [x] 80.3.4 — UDP multicast: getifaddrs, IP_ADD_MEMBERSHIP, loopback filtering, _z_slice_t addr return
  - [x] 80.3.5 — Activate `network` feature for POSIX + remove C unix/network.c from build
  - [x] 80.3.6 — Native integration tests pass: actions (3/3), error_handling (8/8)
- [x] 80.4 — Bare-metal build fixes
  - [x] 80.4.1 — Fixed size probe for ARM: set `ZENOH_GENERIC` + include `c/platform` dir
  - [x] 80.4.2 — Made size probe failure-tolerant (try_compile fallback for RISC-V without picolibc)
  - [x] 80.4.3 — Fixed opaque type alignment (`repr(C)` without hardcoded align — was causing ABI mismatch)
- [x] 80.5 — Create standalone `nros-smoltcp` crate (RMW-agnostic network provider)
  - [x] 80.5.1 — Create `packages/drivers/nros-smoltcp/` with SmoltcpBridge, socket management, staging buffers
  - [x] 80.5.2 — Export RMW-agnostic API: `tcp_open()`, `tcp_read()`, `tcp_send()`, `udp_open()`, etc.
  - [x] 80.5.3 — Export board-facing API: `init()`, `poll()`, `create_and_register_sockets()`, `set_poll_callback()`
  - [x] 80.5.4 — Refactor zpico-smoltcp as thin wrapper over nros-smoltcp (no zpico-sys dependency)
  - [x] 80.5.5 — Board platform crates implement `PlatformTcp`/`PlatformUdp` delegating to nros-smoltcp
  - [x] 80.5.6 — Wire shim `network` feature for bare-metal (replaces `socket-stubs`)
  - [x] 80.5.7 — Verified: `just check` passes, `just qemu test` passes, BSP E2E works
- [x] 80.6 — Migrate board crates from zpico-smoltcp to nros-smoltcp
  - [x] 80.6.1 — Board crates depend on `nros-smoltcp` directly (replace `zpico_smoltcp::*` imports)
  - [x] 80.6.2 — Move `smoltcp_init`/`smoltcp_cleanup`/`smoltcp_poll` FFI to zpico-platform-shim via nros-smoltcp FFI exports
  - [x] 80.6.3 — Remove zpico-smoltcp dependency from all 4 board crates
  - [x] 80.6.4 — Migrate reference examples (`stm32f4-porting/polling`, `stm32f4-porting/rtic`)
  - [x] 80.6.5 — Delete `packages/zpico/zpico-smoltcp/` crate entirely (-2432 lines)
  - [x] 80.6.6 — Remove zpico-smoltcp from workspace exclude list + justfile C format/check
  - [x] 80.6.7 — Remove `socket_stubs` and `smoltcp` features from zpico-platform-shim
- [x] 80.6.8 — Implement `PlatformTcp`/`PlatformUdp`/`PlatformSocketHelpers` for Zephyr
  - [x] 80.6.8.1 — `nros-platform-zephyr/src/net.rs` with manual `extern "C"` bindings for Zephyr POSIX sockets (libc crate doesn't target Zephyr)
  - [x] 80.6.8.2 — Zephyr-specific constants (`AF_INET=1`, `O_NONBLOCK=0x4000`, `SOL_SOCKET=1`, `SO_RCVTIMEO=20`, `SHUT_RDWR=2`) — values verified against `zephyr/net/socket.h` + `zephyr/net/net_ip.h`
  - [x] 80.6.8.3 — TCP (create/free endpoint, open, listen, close, read, read_exact, send) — fully functional
  - [x] 80.6.8.4 — UDP unicast (create/free endpoint, open, close, read, read_exact, send) — fully functional
  - [x] 80.6.8.5 — Socket helpers (set_non_blocking, accept, close, wait_event) — fully functional
  - [x] 80.6.8.6 — UDP multicast — stubbed (returns error). Zephyr examples use `tcp/` locator + `CONFIG_NROS_ZENOH_SCOUTING=n`, multicast is never exercised. Follow-up tracked as 80.6.9.
  - [x] 80.6.8.7 — Activate `zpico-platform-shim?/network` for `zephyr = [...]` in `zpico-sys/Cargo.toml`
  - [x] 80.6.8.8 — Remove `zenoh-pico/src/system/zephyr/network.c` from `zephyr/CMakeLists.txt` (replaced by Rust shim forwarders)
  - [x] 80.6.8.9 — `just zephyr build` clean, `just zephyr test` 23/27 (same 4 pre-existing zeth0 TAP contention failures tracked in Phase 81, zero new regressions)
- Zephyr UDP multicast deferred to 80.11
- [x] 80.7 — FreeRTOS lwIP networking via bindgen
  - [x] 80.7.1 — Create `freertos-lwip-sys` bindgen crate (`packages/drivers/freertos-lwip-sys/`)
  - [x] 80.7.2 — Wire `nros-platform-freertos/net.rs` to use `freertos-lwip-sys` types
  - [x] 80.7.3 — Activate shim `network` for FreeRTOS + remove C `freertos/lwip/network.c`
  - [x] 80.7.4 — Verified: 17/18 builds pass (1 pre-existing C API), pubsub E2E pass
- [x] 80.8 — ThreadX NetX Duo networking (manual FFI, pending bindgen migration)
  - [x] 80.8.1 — `nros-platform-threadx/net.rs` using `nx_bsd_*` manual FFI
  - [x] 80.8.2 — Activate shim `network` for ThreadX + remove C `threadx/network.c`
  - [x] 80.8.3 — Fix `_tx_thread_sleep` link name + C++ `global_handle()` friend decl
  - [x] 80.8.4 — Verified: 16/16 builds pass, Rust pubsub + service E2E pass
- [x] 80.9 — Per-RTOS bindgen sys crates (consistency + safety)
  - [x] 80.9.1 — Create `threadx-netx-sys` bindgen crate (`packages/drivers/threadx-netx-sys/`)
  - [x] 80.9.2 — Wire `nros-platform-threadx/net.rs` to use `threadx-netx-sys` types — found wrong constants (SOL_SOCKET, SO_RCVTIMEO)
  - [x] 80.9.3 — Create `nuttx-sys` bindgen crate — found different constants (SOL_SOCKET=1, SO_RCVTIMEO=10, TCP_NODELAY=16, time_t=u64)
  - [x] 80.9.4 — Create `zephyr-posix-sys` bindgen crate (placeholder fallback — see known issue below)
- [x] 80.10 — Implement for NuttX via nuttx-sys bindgen
  - [x] 80.10.1 — `nros-platform-nuttx/net.rs` using `nuttx-sys` types (NuttxPlatform as proper struct)
  - [x] 80.10.2 — Activate shim `network` for NuttX + remove C `unix/network.c`
  - [x] 80.10.3 — Verified: 20/21 pass (1 C service client failure — Phase 82 API, not networking)
- [ ] 80.11 — Zephyr UDP multicast
  - [ ] 80.11.1 — Port posix mcast_open/listen/read/send to Zephyr
  - [ ] 80.11.2 — Exercise via a Zephyr example with scouting enabled
- [x] 80.12 — XRCE-DDS network unification via nros-platform
  - [x] 80.12.1 — Add `set_recv_timeout()` to `PlatformUdp` trait (XRCE needs per-read timeout)
  - [x] 80.12.2 — Implement `set_recv_timeout` in 5 platform crates (posix, zephyr, freertos, nuttx, threadx)
  - [x] 80.12.3 — Create `platform_udp.rs` in nros-rmw-xrce: XRCE callbacks delegate to `ConcretePlatform::udp_*()`
  - [x] 80.12.4 — Wire `nros-rmw-xrce` to use `platform_udp` module via `platform-udp` feature
  - [x] 80.12.5 — Remove `posix_udp.rs` and `zephyr.rs` from nros-rmw-xrce
  - [x] 80.12.6 — Remove transport callbacks + clock symbols + `xrce_zephyr_init` from `xrce_zephyr.c`
  - [x] 80.12.7 — Add `udp_set_recv_timeout` to 4 bare-metal board crates (mps2-an385, esp32, esp32-qemu, stm32f4) — enables XRCE via platform_udp on bare-metal
  - [x] 80.12.8 — Verify Zephyr XRCE Rust test passes (25/25 + 2 skipped)
  - [x] 80.12.9 — Migrate all 6 C XRCE examples to `nros_support_init_named()`, remove `xrce_zephyr_init`
  - [x] 80.12.10 — Delete `xrce-smoltcp` crate (replaced by nros-smoltcp via PlatformUdp)
  - [x] 80.12.11 — Strip `xrce_zephyr.c` to L4 wait only (transport callbacks + clock symbols removed)
- [x] 80.14 — RMW-agnostic serial transport via nros-platform
  - [x] 80.14.1 — Added `PlatformSerial` trait to `nros-platform-api`
        (`open(path)`, `close()`, `read(buf, len, timeout_ms)`,
        `write(buf, len)`, `configure(baudrate)`). `read` returns
        `0` on timeout (not an error); both XRCE and zenoh-pico
        tolerate it. Documented in the trait rustdoc along with the
        single-active-device-per-process invariant.
  - [x] 80.14.2 — Implemented `PlatformSerial` on `PosixPlatform`.
        New `nros-platform-posix::serial` module, gated
        `#[cfg(not(target_os = "nuttx"))]` (same libc-availability
        carve-out as `net`). Baudrates supported:
        `9_600, 19_200, 38_400, 57_600, 115_200, 230_400, 460_800,
        921_600`.
  - [x] 80.14.3 — New `nros-rmw-xrce::platform_serial` module
        (replaces `posix_serial`). XRCE callbacks dispatch via
        `<ConcretePlatform as PlatformSerial>::*` — platform-agnostic
        from the XRCE backend's perspective. Public entry point:
        `init_platform_serial_transport(device_path)`. Backwards-
        compatible `nros::init_posix_serial(pty_path)` wrapper
        updated to call the new path.
  - [~] 80.14.4 — **Rescoped / deferred**. Original wording
        ("replace `zpico-serial` direct libc") was inaccurate:
        zpico-serial does not use libc — it exposes a `SerialPort`
        trait that board crates implement against their UART
        peripheral, with per-port RX ring buffers. That's already a
        clean platform-layer abstraction, just not named
        `PlatformSerial`. Wiring it through `PlatformSerial` would
        be one layer of indirection reshuffle (SerialPort on a
        board's UART driver → `PlatformSerial` on the board's
        platform ZST → zpico-serial dispatches through
        `ConcretePlatform`) without a clear architectural win. Two
        live users — `nros-mps2-an385` and `nros-stm32f4` — each
        register a UART via `zpico_serial::register_port`; leaving
        that path unchanged. Revisit when a third bare-metal serial
        consumer materialises and the `SerialPort`-vs-
        `PlatformSerial` split starts costing something.
  - [x] 80.14.5 — Deleted `nros-rmw-xrce::posix_serial` (replaced by
        `platform_serial`). Call sites in `nros` / `nros-node`
        switched to `nros_rmw_xrce::platform_serial::init_platform_serial_transport`.
  - [x] 80.14.6 — Verified: 3/3 XRCE serial tests pass
        (`test_xrce_serial_listener_starts`, `_talker_starts`,
        `_communication`); full 14/14 XRCE suite also passes post-
        change.
- [ ] 80.13 — Update documentation
  - [ ] 80.13.1 — Update `book/src/guides/porting-platform/implementing-a-platform.md`
  - [ ] 80.13.2 — Update Phase 79 symbol tables to reflect network unification
  - [ ] 80.13.3 — Update workspace structure in CLAUDE.md (nros-smoltcp, *-sys crates)

## Design Decisions

### Why opaque handles instead of Rust socket traits?

The network functions are called from C code (zenoh-pico's transport layer).
The C code passes platform-specific structs by value. Using opaque `*mut c_void`
handles (or fixed-size `#[repr(C)]` wrappers) keeps the FFI boundary simple.
Rust-side generics and trait objects can't cross `extern "C"` boundaries.

### Why cffi vtable for lwIP/NetX?

lwIP and NetX Duo have C APIs that are most naturally called from C. Wrapping
every `lwip_socket()`, `lwip_connect()`, `lwip_select()` call in Rust FFI
bindings adds complexity without benefit. The cffi vtable lets the board crate
register C function pointers that call the networking stack directly.

POSIX and smoltcp, being available as Rust crates (`libc`, `smoltcp`), are
better implemented directly in Rust.

### Pass-by-value socket handling

zenoh-pico's `_z_read_tcp(sock, ...)` passes the socket struct by value.
The shim uses **compiler-detected sizes** — zpico-sys's build.rs compiles a
C size probe and emits the struct sizes as `DEP_ZPICO_*` variables. The shim
reads these in its own build.rs and generates `#[repr(C)]` types with the
exact size for the target platform. No hardcoded constants, no manual updates.

This is alloc-free: sockets are passed by value on the stack with zero heap
allocation. The size detection works for any cross-compilation target because
it uses the same cross-compiler as the main build — it compiles but never
runs the probe.

### Platform-specific shim (alternative, not used)
   platform with the correct size. This matches exactly but requires
   per-platform shim code.

Option 1 is simpler. The extra bytes (padding) are negligible for function
call overhead.

### Multicast deferred

UDP multicast (`_z_open_udp_multicast`, etc.) is only used for zenoh
scouting and has complex platform-specific behavior (interface selection,
IPv6 group join). It's deferred to a later phase. The initial implementation
covers TCP + UDP unicast which handles all zenoh client-mode communication.

## Acceptance Criteria

- [ ] `just test-integration` passes (POSIX networking via nros-platform-posix)
- [ ] `just test-qemu` passes (bare-metal networking via nros-platform-<board>)
- [ ] `just test-freertos` passes (lwIP networking via cffi vtable)
- [ ] `just test-threadx` passes (NetX networking via cffi vtable)
- [ ] No C `network.c` files compiled in zpico-sys for any platform
- [ ] Board crates have zero zpico-specific networking code
- [ ] Adding a new networking stack requires only an nros-platform implementation

## Notes

- Serial transport (`_z_open_serial`, etc.) is separate from TCP/UDP and
  handled by `zpico-serial` / zenoh-pico's built-in serial. Not included
  in this phase.
- TLS (`_z_open_tls`) is an extension of TCP with certificate management.
  Deferred — current TLS support via mbedTLS is POSIX-only.
- XRCE-DDS uses custom transport callbacks (`uxrCustomTransport`), not the
  zenoh-pico socket interface. Phase 80.8 evaluates whether XRCE can benefit
  from unified networking or should stay with its existing callback model.

## Known Issues / Future Work

### Busy-loop polling in `PlatformTcp::tcp_open()`

The smoltcp `tcp_open` implementation uses a tight busy-loop calling
`SmoltcpBridge::poll_network()` + `clock_now_ms()` until the TCP connection
is established or times out. This wastes CPU cycles and is not suitable for
battery-powered devices. Future work should use interrupt-driven or
waker-based connection establishment:

- ARM WFI + timer interrupt to wake on smoltcp poll interval
- smoltcp's `poll_delay()` to compute the next wakeup time
- RTIC async tasks with proper waker integration

### Transport preference: serial or ethernet over WiFi

Board crates should default to serial or ethernet transport where available.
WiFi introduces additional complexity (credential management, power
management, scanning) that is orthogonal to the nros-platform abstraction.
ESP32 boards should prefer ethernet (via QEMU OpenETH) for testing and
serial for physical devices without Ethernet hardware.

### RTIC networked E2E test reliability

RTIC QEMU E2E tests (pubsub, service, action) intermittently fail with
`Transport(ConnectionFailed)` in the nextest harness despite working when
run manually. Likely a timing issue: the test harness 15-second timeout
may be insufficient for QEMU slirp + smoltcp TCP handshake under load.
Stale build artifacts can also cause failures — clean rebuilds resolve
this. Consider increasing timeout or adding explicit connection-ready
detection in the test harness.

### `zephyr-posix-sys` bindgen — build tree path mismatch

`zephyr-posix-sys` extracts include paths from a Zephyr build tree's
`compile_commands.json`. Currently bindgen fails with "processor
architecture not supported" because:

1. Zephyr headers require `CONFIG_ARCH_POSIX` (defined in `autoconf.h`)
2. `autoconf.h` is found via include paths in `compile_commands.json`
3. Those paths may point to a **different directory** than where the
   actual generated headers live (e.g., `zephyr-workspace/build-talker/`
   vs `nano-ros-workspace/build-talker/`)
4. Additionally, clang (used by bindgen) needs `--target=x86_64-linux-gnu`
   and the Zephyr gcc.h toolchain header checks arch-specific macros

To fix: either rewrite `compile_commands.json` paths at extraction time,
or generate a minimal "bindgen shim" header that `#include`s `autoconf.h`
explicitly with corrected paths. The crate falls back to hand-verified
placeholder bindings (matching the current `nros-platform-zephyr/net.rs`
manual FFI values) when bindgen fails.
