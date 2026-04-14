# Phase 80: Unified Network Interface for nros-platform

**Goal**: Extend the nros-platform abstraction to cover networking (TCP/UDP socket operations), making the RMW transport layer fully platform-agnostic.

**Status**: Not Started
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
  - [ ] 80.2.4 — Remove existing `socket_stubs` module (deferred — replaced when `network` feature activated)
- [x] 80.3 — Implement `PlatformTcp`/`PlatformUdp` for POSIX
  - [x] 80.3.1 — TCP: getaddrinfo, socket, connect, recv, send, shutdown+close via libc
  - [x] 80.3.2 — UDP: getaddrinfo, socket, recvfrom, sendto via libc
  - [x] 80.3.3 — Socket helpers: fcntl(O_NONBLOCK), accept, socket_close, wait_event
  - [-] 80.3.4 — ~~Activate `network` feature~~ (blocked — UDP multicast not yet implemented; C network.c kept)
- [ ] 80.3.5 — Implement UDP multicast for POSIX (6 functions: open, listen, close, read, read_exact, send)
- [ ] 80.3.6 — Activate `network` feature for POSIX + verify `just test-integration` passes
- [ ] 80.4 — Implement for bare-metal (smoltcp)
  - [ ] 80.4.1 — Move zpico-smoltcp TCP/UDP logic into nros-platform-<board> or a shared crate
  - [ ] 80.4.2 — Verify `just test-qemu` passes
- [ ] 80.5 — Implement for FreeRTOS (lwIP) via cffi vtable
  - [ ] 80.5.1 — C vtable provides lwIP socket functions
  - [ ] 80.5.2 — Board crate registers vtable during init
  - [ ] 80.5.3 — Verify `just test-freertos` passes (29/29)
- [ ] 80.6 — Implement for ThreadX (NetX Duo) via cffi vtable
  - [ ] 80.6.1 — C vtable provides NetX BSD socket functions
  - [ ] 80.6.2 — Verify `just test-threadx` passes
- [ ] 80.7 — Skip C `network.c` compilation in zpico-sys
  - [ ] 80.7.1 — Remove `unix/network.c` from CMake build copy (POSIX)
  - [ ] 80.7.2 — Remove `freertos/lwip/network.c` from cc build (FreeRTOS)
  - [ ] 80.7.3 — Remove `threadx/network.c` from cc build (ThreadX)
  - [ ] 80.7.4 — Bare-metal: remove zpico-smoltcp dependency from zpico-sys
- [ ] 80.8 — Add network functions to xrce-platform-shim (if applicable)
  - [ ] 80.8.1 — Check if XRCE-DDS uses the same network interface or custom transport
- [ ] 80.9 — Extend nros-platform-cffi vtable with network fields
- [ ] 80.10 — Update documentation
  - [ ] 80.10.1 — Update `book/src/guides/porting-platform/implementing-a-platform.md`
  - [ ] 80.10.2 — Update Phase 79 symbol tables to reflect network unification

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
