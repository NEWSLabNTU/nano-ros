# Zephyr native_sim: glibc Symbol Collision Pitfall

## Problem

On Zephyr `native_sim`, Rust FFI `extern "C"` declarations for standard BSD socket functions (`socket`, `connect`, `getaddrinfo`, `send`, `recv`, etc.) resolve to **glibc** symbols instead of Zephyr's NSOS (Native Sim Offloaded Sockets) implementations.

This causes:

1. **`addrinfo` layout mismatch** — glibc's `getaddrinfo` returns POSIX `struct addrinfo` (with `ai_flags` first), but Zephyr's `zsock_addrinfo` has `ai_next` first. Reading the wrong field offsets produces garbage values (`ai_family=6, ai_socktype=16` instead of `2, 2`).

2. **Host sockets instead of NSOS sockets** — glibc's `socket()` creates real host kernel sockets that bypass Zephyr's network stack entirely. These sockets don't go through NSOS's epoll-based offload mechanism.

3. **Silent failures** — the glibc `socket()` call may succeed (creating a host socket), but subsequent operations fail because the socket isn't managed by Zephyr's socket offload table. The process exits immediately with no error message.

## Root Cause

Zephyr's `native_sim` target runs as a Linux executable. The final binary links against:
- **Zephyr's static libraries** (kernel, drivers, NSOS)
- **glibc** (via the host toolchain's runtime)

Zephyr provides POSIX-compatible socket functions via two mechanisms:
- `zsock_*` functions (e.g., `zsock_socket`, `zsock_getaddrinfo`) — always available
- POSIX name macros (`#define socket zsock_socket`) — only active when `CONFIG_NET_SOCKETS_POSIX_NAMES=y`

The POSIX name macros work for **C code compiled by Zephyr's CMake** (because the Zephyr headers are included). But **Rust FFI `extern "C"` declarations** bypass these macros — they emit direct symbol references to `socket`, `getaddrinfo`, etc., which resolve to glibc at link time.

## Why zenoh-pico Works

zenoh-pico's C code is compiled by Zephyr's CMake build system with Zephyr headers included. The `#define socket zsock_socket` macro redirects all calls to Zephyr's NSOS layer. No Rust FFI is involved in the zenoh-pico transport path.

## Why XRCE Was Affected

The Phase 80.12 XRCE unification routes transport calls through `nros-platform-zephyr` (Rust) → `extern "C" { fn socket(...) }` → **glibc**. Before unification, `xrce_zephyr.c` (compiled by Zephyr CMake) called `zsock_socket` directly.

## Fix

Add C shim wrappers in `zephyr/nros_platform_zephyr_shims.c` that call the `zsock_*` API explicitly:

```c
int nros_zephyr_socket(int family, int type, int proto) {
    return zsock_socket(family, type, proto);
}

int nros_zephyr_getaddrinfo(const char *node, const char *service,
                            const struct zsock_addrinfo *hints,
                            struct zsock_addrinfo **res) {
    return zsock_getaddrinfo(node, service, hints, res);
}

// ... same for connect, bind, send, recv, close, setsockopt, etc.
```

Then use `#[link_name]` in the Rust FFI declarations (`nros-platform-zephyr/src/net.rs`):

```rust
unsafe extern "C" {
    #[link_name = "nros_zephyr_socket"]
    pub fn socket(family: c_int, ty: c_int, proto: c_int) -> c_int;

    #[link_name = "nros_zephyr_getaddrinfo"]
    pub fn getaddrinfo(...) -> c_int;

    // ... all BSD socket functions
}
```

## Scope

This affects **any Rust code** that calls BSD socket functions via FFI on Zephyr `native_sim`. It does NOT affect:
- C code compiled by Zephyr CMake (macros handle the redirect)
- Real hardware targets (no glibc, only Zephyr's libc)
- Non-socket FFI (kernel APIs like `k_uptime_get` are static inlines wrapped in separate shims)

## Diagnostic Symptoms

- Process exits immediately after session/socket init with no error log
- `getaddrinfo` returns a valid pointer but `ai_family`/`ai_socktype` fields contain garbage
- `socket()` returns a valid fd but the socket doesn't appear in Zephyr's socket table
- `nm binary | grep " U.*@GLIBC"` shows `socket@GLIBC_2.2.5`, `getaddrinfo@GLIBC_2.2.5`, etc.

## Files

- `zephyr/nros_platform_zephyr_shims.c` — C shim wrappers (`nros_zephyr_socket`, etc.)
- `packages/core/nros-platform-zephyr/src/net.rs` — Rust FFI with `#[link_name]` attributes
- `zephyr/CMakeLists.txt` — shims compiled for all RMW backends (not just zenoh)

## Related

- Phase 80.12 (XRCE network unification) — discovered the issue
- `docs/research/zephyr-native-sim-timing.md` — other native_sim pitfalls
