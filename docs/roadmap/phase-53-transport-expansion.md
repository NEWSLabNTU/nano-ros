# Phase 53 — Transport Layer Expansion (UDP + TLS)

## Context

nano-ros initially only supported TCP for the zenoh-pico transport layer. This phase adds UDP and TLS transport support for both native/POSIX and bare-metal platforms.

**Principles:**
- Use zenoh-pico's native transport implementations on POSIX; only use smoltcp on bare-metal
- TLS layers on top of TCP — reuse existing TCP infrastructure
- mbedTLS for both platforms (zenoh-pico already uses it on POSIX)

## Progress

### UDP Transport

- [x] 53.1 — Feature forwarding chain for `link-udp-unicast`
- [x] 53.2 — smoltcp UDP socket infrastructure in `zpico-smoltcp`
- [x] 53.3 — UDP platform symbols (`udp.rs` + `util.rs`)
- [x] 53.4 — Board crate UDP socket registration
- [x] 53.5 — Native example: verify UDP locator
- [x] 53.6 — Bare-metal example: UDP on QEMU ARM
- [x] 53.7 — UDP documentation

### TLS Transport

- [ ] 53.8 — Feature forwarding chain for `link-tls`
- [ ] 53.9 — POSIX TLS: enable zenoh-pico's built-in mbedTLS support
- [ ] 53.10 — Native example: verify TLS locator
- [ ] 53.11 — Bare-metal platform header: add `_tls_sock` field
- [ ] 53.12 — mbedTLS build integration for bare-metal
- [ ] 53.13 — Bare-metal TLS platform symbols (`tls.rs`)
- [ ] 53.14 — Bare-metal example: TLS on QEMU ARM
- [ ] 53.15 — TLS documentation

---

## UDP Deliverables (Complete)

### 53.1 — Feature forwarding chain for `link-udp-unicast` ✓

`zpico-sys` already defines `link-udp-unicast`. Forward it up the chain:

- `packages/zpico/nros-rmw-zenoh/Cargo.toml` — add `link-udp-unicast = ["zpico-sys/link-udp-unicast"]`
- `packages/core/nros/Cargo.toml` — add `link-udp-unicast = ["nros-rmw-zenoh?/link-udp-unicast"]`

### 53.2 — smoltcp UDP socket infrastructure in `zpico-smoltcp` ✓

**`packages/zpico/zpico-smoltcp/Cargo.toml`:**
- Add `"socket-udp"` to smoltcp features
- Add `"link-udp-unicast"` to zpico-sys features

**`packages/zpico/zpico-smoltcp/src/bridge.rs`:**
- Add separate `UdpSocketEntry` struct + `UDP_SOCKET_TABLE` (independent from TCP table)
- Add `MAX_UDP_SOCKETS` constant (default 2, configurable via `ZPICO_SMOLTCP_MAX_UDP_SOCKETS`)
- Add `UDP_SOCKET_RX_BUFFERS` / `UDP_SOCKET_TX_BUFFERS` staging buffers
- Add bridge methods: `udp_socket_open()`, `udp_socket_close()`, `udp_socket_send(handle, data, ip, port)`, `udp_socket_recv(handle, buf)`
- Extend `SmoltcpBridge::poll()` to process UDP sockets (no connection state machine — just transfer RX/TX data between staging buffers and smoltcp UdpSocket, auto-bind on first poll)
- Key difference: UDP send takes per-packet endpoint (sendto semantics)

**`packages/zpico/zpico-smoltcp/src/lib.rs`:**
- Add static UDP packet metadata + data buffer arrays (smoltcp's `PacketMetadata`/`PacketBuffer`)
- Add `create_and_register_udp_sockets()` function
- Re-export UDP socket types from smoltcp
- `TOTAL_SOCKETS` = `MAX_SOCKETS` + `MAX_UDP_SOCKETS` for socket storage sizing

**`packages/zpico/zpico-smoltcp/build.rs`:**
- Add `ZPICO_SMOLTCP_MAX_UDP_SOCKETS` env var (default 2)

### 53.3 — UDP platform symbols (`udp.rs`) ✓

**`packages/zpico/zpico-smoltcp/src/udp.rs`** (new file):

8 `#[unsafe(no_mangle)]` extern "C" functions matching zenoh-pico's `udp.h`:
- `_z_create_endpoint_udp` / `_z_free_endpoint_udp` — parse IP+port (reuse helpers)
- `_z_open_udp_unicast` — allocate from UDP bridge table, bind local port, no connection wait
- `_z_listen_udp_unicast` — return error (not supported in client mode)
- `_z_close_udp_unicast` — release bridge socket
- `_z_read_udp_unicast` / `_z_read_exact_udp_unicast` — read from staging buffer with timeout+poll
- `_z_send_udp_unicast` — write to staging buffer with per-packet endpoint

**`packages/zpico/zpico-smoltcp/src/util.rs`** (new file):
- Extract `parse_ip_address()` and `parse_port()` from `tcp.rs` into shared module

### 53.4 — Board crate UDP socket registration ✓

Update board crates to create and register UDP sockets alongside TCP:
- `packages/boards/nros-mps2-an385/src/node.rs`
- `packages/boards/nros-stm32f4/src/node.rs`
- `packages/boards/nros-esp32/src/node.rs`
- `packages/boards/nros-esp32-qemu/src/node.rs`

Each `init_network()` function adds `create_and_register_udp_sockets()` call after TCP socket setup.
Board crate `Cargo.toml` files add `"socket-udp"` to their smoltcp dependency features.

### 53.5 — Native example: verify UDP locator ✓

Verified native talker/listener works with `ZENOH_LOCATOR=udp/127.0.0.1:7447`.
Added UDP transport documentation to native example doc comments (talker + listener `main.rs`).

No Cargo.toml changes needed — on POSIX, zenoh-pico has built-in UDP support via OS sockets.

### 53.6 — Bare-metal example: UDP on QEMU ARM ✓

Added `link-udp-unicast` feature to QEMU ARM talker and listener `Cargo.toml` files.
Both examples build successfully with `--target thumbv7m-none-eabi`.

To test with UDP, use `Config::default().with_zenoh_locator("udp/192.0.3.1:7447")`.

### 53.7 — UDP documentation ✓

- `docs/reference/environment-variables.md` — UDP locator format, `ZPICO_SMOLTCP_MAX_UDP_SOCKETS`
- `docs/guides/quick-reference.md` — UDP transport section

---

## TLS Design

### Architecture

TLS in zenoh-pico layers on top of TCP. The `_z_open_tls()` function first opens a TCP connection via `_z_open_tcp()`, then performs a TLS handshake over it using mbedTLS. All subsequent reads/writes go through `mbedtls_ssl_read()`/`mbedtls_ssl_write()`, which internally call BIO callbacks to transfer encrypted data over the underlying TCP socket.

```
Application (zenoh-pico)
    │
    ▼
_z_read_tls / _z_write_tls          ← TLS platform symbols
    │
    ▼
mbedtls_ssl_read / mbedtls_ssl_write ← mbedTLS protocol engine
    │
    ▼
BIO callbacks (f_send / f_recv)      ← pluggable I/O layer
    │
    ├─ POSIX:      send(fd) / recv(fd)
    └─ Bare-metal: SmoltcpBridge::socket_send() / socket_recv()
```

### BIO Callback Contract

mbedTLS uses `mbedtls_ssl_set_bio()` to register custom I/O callbacks:

```c
mbedtls_ssl_set_bio(&ssl, context, f_send, f_recv, NULL);
```

- **`f_send(ctx, buf, len)`** → returns bytes sent, or `MBEDTLS_ERR_SSL_WANT_WRITE`
- **`f_recv(ctx, buf, len)`** → returns bytes read, or `MBEDTLS_ERR_SSL_WANT_READ`

On POSIX, `ctx` is `&sock._fd` (file descriptor). On bare-metal, `ctx` will be a pointer to the smoltcp socket handle. The smoltcp bridge's non-blocking `socket_send()`/`socket_recv()` return 0 when the buffer is full/empty, which maps directly to `MBEDTLS_ERR_SSL_WANT_WRITE`/`MBEDTLS_ERR_SSL_WANT_READ`.

### POSIX vs Bare-Metal

**POSIX**: zenoh-pico already has a complete TLS implementation in `src/system/unix/tls.c` (706 lines). It uses POSIX `send()`/`recv()` as BIO callbacks and links against system mbedTLS. Just enable the feature flag.

**Bare-metal**: zenoh-pico has no `tls.c` for bare-metal. We must provide the 9 TLS platform symbols in `zpico-smoltcp/src/tls.rs`, using custom BIO callbacks that route through the smoltcp TCP bridge. mbedTLS itself must be cross-compiled and linked for the target.

### Bare-Metal Platform Header Change

The bare-metal `_z_sys_net_socket_t` currently has no `_tls_sock` field:

```c
// Current (no TLS support):
typedef struct {
    int8_t _handle;
    bool _connected;
} _z_sys_net_socket_t;

// With TLS:
typedef struct {
    int8_t _handle;
    bool _connected;
#if Z_FEATURE_LINK_TLS == 1
    void *_tls_sock;  // Pointer to _z_tls_socket_t (same as POSIX)
#endif
} _z_sys_net_socket_t;
```

This is needed because zenoh-pico's TLS link layer stores a back-pointer from the underlying TCP socket to the TLS context for the `_read_socket_f` callback.

### Memory Budget

mbedTLS on bare-metal requires approximately:
- ~60 KB ROM (code + read-only data)
- ~1–36 KB RAM per TLS session (depends on cipher suite and buffer sizes)
- Certificate storage (CA cert in flash, parsed structures in RAM)

For Cortex-M targets with limited RAM, use PSK (pre-shared key) mode to avoid certificate parsing overhead (~30 KB less than ECDHE).

### TLS Configuration

zenoh-pico supports 12 TLS config parameters. For bare-metal client mode, only these are needed:

| Parameter | Purpose |
|-----------|---------|
| `root_ca_certificate_base64` | CA cert (base64-encoded, stored in flash) |
| `verify_name_on_connect` | Hostname verification (default: true) |
| `enable_mtls` | Mutual TLS (optional) |
| `connect_private_key_base64` | Client key for mTLS (optional) |
| `connect_certificate_base64` | Client cert for mTLS (optional) |

File-path variants (`root_ca_certificate`, etc.) are not available on bare-metal (no filesystem).

---

## TLS Deliverables

### 53.8 — Feature forwarding chain for `link-tls`

Add `link-tls` feature flag and forward through the crate chain:

**`packages/zpico/zpico-sys/Cargo.toml`:**
- Add `link-tls = []` to `[features]`

**`packages/zpico/zpico-sys/build.rs`:**
- Add `tls` field to `LinkFeatures` struct
- Add `tls_flag()` method
- Change `Z_FEATURE_LINK_TLS` from hardcoded `0` to `link.tls_flag()`

**`packages/zpico/nros-rmw-zenoh/Cargo.toml`:**
- Add `link-tls = ["zpico-sys/link-tls"]`

**`packages/core/nros/Cargo.toml`:**
- Add `link-tls = ["nros-rmw-zenoh?/link-tls"]`

**Acceptance criteria:**
- `Z_FEATURE_LINK_TLS` is `1` when `link-tls` feature is enabled, `0` otherwise
- No regressions: `just quality` passes without `link-tls` enabled

### 53.9 — POSIX TLS: enable zenoh-pico's built-in mbedTLS support

On POSIX, zenoh-pico's `src/system/unix/tls.c` provides the complete TLS implementation. Enable it by linking mbedTLS.

**`packages/zpico/zpico-sys/build.rs`:**
- When `link-tls` is enabled on POSIX targets: add mbedTLS include paths and link flags
- Use `pkg-config` to find mbedTLS (matching zenoh-pico's CMakeLists.txt approach)
- Link `mbedtls`, `mbedx509`, `mbedcrypto`

**`packages/zpico/zpico-sys/Cargo.toml`:**
- Add `pkg-config` to build-dependencies (for finding mbedTLS)

**Acceptance criteria:**
- `cargo build -p zpico-sys --features "posix,link-tcp,link-tls"` succeeds
- Native talker builds with `link-tls` feature
- System package requirement: `libmbedtls-dev` (documented, not auto-installed)

### 53.10 — Native example: verify TLS locator

Verify native talker/listener works with TLS:

```bash
# Generate test certificates
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"

# Start zenohd with TLS
zenohd --listen tls/localhost:7447 \
  --cfg "transport/tls/listen_certificate:cert.pem" \
  --cfg "transport/tls/listen_private_key:key.pem"

# Run with TLS locator
ZENOH_LOCATOR=tls/localhost:7447 cargo run -p native-rs-talker
```

Add TLS documentation to native example doc comments.

**Acceptance criteria:**
- Native talker/listener exchange messages over TLS
- Doc comments updated with TLS usage instructions

### 53.11 — Bare-metal platform header: add `_tls_sock` field

**`packages/zpico/zpico-sys/c/platform/zenoh_bare_metal_platform.h`:**
- Add `void *_tls_sock;` field to `_z_sys_net_socket_t` behind `#if Z_FEATURE_LINK_TLS == 1`

**`packages/zpico/zpico-smoltcp/src/tcp.rs`:**
- Add `_tls_sock` field to `ZSysNetSocket` repr(C) struct behind `#[cfg(feature = "link-tls")]`
- Ensure field is initialized to null in `_z_open_tcp()`

**Acceptance criteria:**
- Existing TCP-only builds are unaffected (field only present when `link-tls` enabled)
- `ZSysNetSocket` layout matches C `_z_sys_net_socket_t` when `link-tls` is enabled
- `just quality` passes

### 53.12 — mbedTLS build integration for bare-metal

Cross-compile mbedTLS for `thumbv7m-none-eabi` and link into zpico-smoltcp.

**`packages/zpico/zpico-smoltcp/Cargo.toml`:**
- Add optional `link-tls` feature: `link-tls = ["zpico-sys/link-tls"]`
- Add `cc` build dependency (for compiling mbedTLS sources)

**`packages/zpico/zpico-smoltcp/build.rs`:**
- When `link-tls` enabled: compile mbedTLS sources with `cc` crate
- Use mbedTLS's `config.h` customized for bare-metal (no filesystem, no threading, no net_sockets)
- Provide custom `mbedtls_platform_*` hooks (calloc/free via bump allocator or similar)
- Configure minimal cipher suite (TLS 1.2 + AES-128-GCM + ECDHE-ECDSA or PSK)

**Acceptance criteria:**
- `cargo build -p zpico-smoltcp --features "link-tls" --target thumbv7m-none-eabi` succeeds
- mbedTLS code size < 80 KB (ROM budget)
- No `std` or libc dependencies from mbedTLS

### 53.13 — Bare-metal TLS platform symbols (`tls.rs`)

**`packages/zpico/zpico-smoltcp/src/tls.rs`** (new file):

9 `#[unsafe(no_mangle)]` extern "C" functions matching zenoh-pico's `tls.h`:

- `_z_tls_context_new` / `_z_tls_context_free` — allocate/free mbedTLS context (static pool or heap)
- `_z_open_tls` — open TCP via bridge, configure mbedTLS with custom BIO callbacks, perform handshake with poll loop
- `_z_listen_tls` — return error (not supported in client mode)
- `_z_tls_accept` — return error (not supported in client mode)
- `_z_close_tls` — close notify + free context + close TCP
- `_z_read_tls` / `_z_write_tls` / `_z_write_all_tls` — delegate to `mbedtls_ssl_read()`/`mbedtls_ssl_write()`

**BIO callbacks** (static functions):
- `tls_bio_send(ctx, buf, len)` — call `SmoltcpBridge::socket_send()`, return `MBEDTLS_ERR_SSL_WANT_WRITE` when buffer full
- `tls_bio_recv(ctx, buf, len)` — call `SmoltcpBridge::socket_recv()`, return `MBEDTLS_ERR_SSL_WANT_READ` when buffer empty

**TLS handshake loop** must interleave `SmoltcpBridge::poll_network()` calls:
```
loop {
    poll_network();
    ret = mbedtls_ssl_handshake(&ssl);
    if ret == 0 { break; }          // success
    if ret == WANT_READ/WRITE { continue; }
    return error;
}
```

**Acceptance criteria:**
- All 9 TLS platform symbols resolve at link time
- TLS handshake completes over smoltcp TCP in QEMU
- No heap allocation (use static TLS context pool, sized by `MAX_TLS_SOCKETS` = 1)

### 53.14 — Bare-metal example: TLS on QEMU ARM

Add `link-tls` feature to a QEMU ARM example and test with `ZENOH_LOCATOR=tls/<bridge-ip>:7447`.

**Example changes:**
- `examples/qemu-arm/rust/zenoh/talker/Cargo.toml` — add `link-tls` to nros features
- Use `Config::default().with_zenoh_locator("tls/192.0.3.1:7447")`
- Embed CA certificate (base64) in the example binary

**Test procedure:**
1. Generate test CA + server cert
2. Start zenohd with TLS listener on bridge IP
3. Run QEMU talker with TLS locator
4. Verify connection + message exchange

**Acceptance criteria:**
- QEMU ARM talker connects to zenohd over TLS and publishes messages
- Binary size increase < 100 KB compared to TCP-only

### 53.15 — TLS documentation

- `docs/reference/environment-variables.md` — TLS locator format, TLS config env vars
- `docs/guides/quick-reference.md` — TLS transport section with certificate generation
- Document mbedTLS system dependency for POSIX builds

**Acceptance criteria:**
- All TLS-related env vars and locator formats documented
- Certificate generation instructions included

---

## Implementation Order

**UDP (complete):** 53.1 → 53.2 → 53.3 → 53.4 → 53.5 → 53.6 → 53.7

**TLS:** 53.8 → 53.9 → 53.10 → 53.11 → 53.12 → 53.13 → 53.14 → 53.15

53.8–53.10 (POSIX TLS) can proceed independently of 53.11–53.14 (bare-metal TLS).

## Key Files

### UDP (complete)

| File | Change |
|------|--------|
| `packages/core/nros/Cargo.toml` | Add `link-udp-unicast` feature |
| `packages/zpico/nros-rmw-zenoh/Cargo.toml` | Add `link-udp-unicast` feature |
| `packages/zpico/zpico-smoltcp/Cargo.toml` | Add `socket-udp`, `link-udp-unicast` |
| `packages/zpico/zpico-smoltcp/src/bridge.rs` | Separate UDP socket table + poll loop |
| `packages/zpico/zpico-smoltcp/src/udp.rs` | **New** — UDP platform symbols |
| `packages/zpico/zpico-smoltcp/src/util.rs` | **New** — shared parse helpers |
| `packages/zpico/zpico-smoltcp/src/tcp.rs` | Use shared util module |
| `packages/zpico/zpico-smoltcp/src/lib.rs` | UDP buffer statics + registration |
| `packages/zpico/zpico-smoltcp/build.rs` | MAX_UDP_SOCKETS config |
| `packages/boards/nros-*/src/node.rs` | UDP socket registration in `init_network()` |
| `packages/boards/nros-*/Cargo.toml` | Add `socket-udp` to smoltcp features |

### TLS

| File | Change |
|------|--------|
| `packages/zpico/zpico-sys/Cargo.toml` | Add `link-tls` feature |
| `packages/zpico/zpico-sys/build.rs` | TLS feature flag + mbedTLS linking (POSIX) |
| `packages/zpico/nros-rmw-zenoh/Cargo.toml` | Add `link-tls` feature forwarding |
| `packages/core/nros/Cargo.toml` | Add `link-tls` feature forwarding |
| `packages/zpico/zpico-sys/c/platform/zenoh_bare_metal_platform.h` | Add `_tls_sock` field |
| `packages/zpico/zpico-smoltcp/Cargo.toml` | Add `link-tls` feature, `cc` dep |
| `packages/zpico/zpico-smoltcp/build.rs` | mbedTLS cross-compilation |
| `packages/zpico/zpico-smoltcp/src/tls.rs` | **New** — TLS platform symbols + BIO callbacks |
| `packages/zpico/zpico-smoltcp/src/tcp.rs` | Add `_tls_sock` to `ZSysNetSocket` |

## Verification

### UDP (complete)

1. ~~`just quality` passes~~
2. ~~Native talker/listener works with `ZENOH_LOCATOR=udp/127.0.0.1:7447`~~
3. ~~QEMU ARM example builds and runs with `link-udp-unicast`~~

### TLS

1. `just quality` passes (with and without `link-tls`)
2. Native talker/listener works with `ZENOH_LOCATOR=tls/localhost:7447`
3. QEMU ARM example connects and publishes over TLS
4. Binary size delta < 100 KB for bare-metal TLS
5. No regressions when `link-tls` is not enabled
