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

- [x] 53.8 — Feature forwarding chain for `link-tls`
- [x] 53.9 — POSIX TLS: enable zenoh-pico's built-in mbedTLS support
- [x] 53.10 — Native example: verify TLS locator
- [x] 53.11 — Bare-metal platform header: add `_tls_sock` field
- [x] 53.12 — mbedTLS build integration for bare-metal
- [x] 53.13 — Bare-metal TLS platform symbols (`tls_bare_metal.c`)
- [x] 53.14 — Bare-metal example: TLS on QEMU ARM
- [x] 53.15 — TLS documentation
- [x] 53.16 — TLS integration test

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
- `packages/boards/nros-board-mps2-an385/src/node.rs`
- `packages/boards/nros-board-stm32f4/src/node.rs`
- `packages/boards/nros-board-esp32/src/node.rs`
- `packages/boards/nros-board-esp32-qemu/src/node.rs`

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

| Parameter                    | Purpose                                   |
|------------------------------|-------------------------------------------|
| `root_ca_certificate_base64` | CA cert (base64-encoded, stored in flash) |
| `verify_name_on_connect`     | Hostname verification (default: true)     |
| `enable_mtls`                | Mutual TLS (optional)                     |
| `connect_private_key_base64` | Client key for mTLS (optional)            |
| `connect_certificate_base64` | Client cert for mTLS (optional)           |

File-path variants (`root_ca_certificate`, etc.) are not available on bare-metal (no filesystem).

---

## TLS Deliverables

### 53.8 — Feature forwarding chain for `link-tls` ✓

Added `link-tls` feature flag and forwarded through the crate chain:

- `packages/zpico/zpico-sys/Cargo.toml` — added `link-tls = []`
- `packages/zpico/zpico-sys/build.rs` — added `tls` to `LinkFeatures`, `Z_FEATURE_LINK_TLS` now dynamic
- `packages/zpico/nros-rmw-zenoh/Cargo.toml` — added `link-tls = ["zpico-sys/link-tls"]`
- `packages/core/nros/Cargo.toml` — added `link-tls = ["nros-rmw-zenoh?/link-tls"]`

`just quality` passes without `link-tls` enabled.

### 53.9 — POSIX TLS: enable zenoh-pico's built-in mbedTLS support ✓

On POSIX, zenoh-pico's `src/system/unix/tls.c` provides the complete TLS implementation.

**`packages/zpico/zpico-sys/build.rs`:**
- `build_zenoh_pico_native()` passes `-DZ_FEATURE_LINK_TLS=1` to CMake when `link-tls` enabled
- CMake's own `FindPkgConfig` handles finding mbedTLS (pkg-config)
- After CMake build: links `mbedtls`, `mbedx509`, `mbedcrypto` via `cargo:rustc-link-lib`
- TLS define also propagated to `build_c_shim()` and `build_zenoh_pico_embedded()`

**System package requirement:** `libmbedtls-dev` (install with `sudo apt install libmbedtls-dev`)

### 53.10 — Native example: verify TLS locator ✓

Verified native talker/listener exchange messages over TLS (14 messages).

**Changes made:**

- `packages/zpico/zpico-sys/build.rs` — Generate pkg-config `.pc` files for mbedTLS
  (Ubuntu's `libmbedtls-dev` doesn't ship them; zenoh-pico's CMake uses `pkg_check_modules`)
- `packages/zpico/zpico-sys/c/shim/zenoh_shim.c` — Map TLS property keys
  (`root_ca_certificate`, `root_ca_certificate_base64`, `verify_name_on_connect`)
  to zenoh-pico `Z_CONFIG_TLS_*` constants
- `packages/zpico/nros-rmw-zenoh/src/shim.rs` — Add env var mappings
  (`ZENOH_TLS_ROOT_CA_CERTIFICATE`, `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64`,
  `ZENOH_TLS_VERIFY_NAME_ON_CONNECT`); increase property buffer to 256 bytes
- `examples/native/rust/zenoh/{talker,listener}/Cargo.toml` — Add `link-tls` feature
- `examples/native/rust/zenoh/{talker,listener}/src/main.rs` — TLS doc comments

**Usage:**

```bash
# Generate test certificate
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"

# Start zenohd with TLS
zenohd --no-multicast-scouting --listen tls/localhost:7447 \
  --cfg 'transport/link/tls/listen_certificate:"cert.pem"' \
  --cfg 'transport/link/tls/listen_private_key:"key.pem"'

# Run with TLS locator
ZENOH_LOCATOR=tls/localhost:7447 \
  ZENOH_TLS_ROOT_CA_CERTIFICATE=cert.pem \
  cargo run -p native-rs-talker --features link-tls
```

### 53.11 — Bare-metal platform header: add `_tls_sock` field ✓

**`packages/zpico/zpico-sys/c/platform/zenoh_bare_metal_platform.h`:**
- Added `void *_tls_sock;` field behind `#if Z_FEATURE_LINK_TLS == 1`
- Used anonymous union for `_handle`/`_fd` compatibility (zenoh-pico's TLS link layer references `_fd`)

**`packages/zpico/zpico-smoltcp/src/tcp.rs`:**
- Added `_tls_sock: *mut core::ffi::c_void` behind `#[cfg(feature = "link-tls")]`
- Initialized to `core::ptr::null_mut()` in `_z_open_tcp()`

**`packages/zpico/zpico-smoltcp/Cargo.toml`:**
- Added `link-tls = ["zpico-sys/link-tls"]` feature

### 53.12 — mbedTLS build integration for bare-metal ✓

**Git submodule:** `packages/zpico/zpico-sys/mbedtls/` → mbedTLS v2.28.9 LTS

**`packages/zpico/zpico-sys/build.rs` (`build_zenoh_pico_embedded()`):**
- When `link-tls`: adds mbedTLS include paths, compiles mbedTLS library sources
  (excluding `net_sockets.c`, `timing.c`, `threading.c`, `psa_its_file.c`)
- Compiles `tls_bare_metal.c` and `entropy_bare_metal.c` from `zpico-smoltcp/c/`
- All compiled into the same `libzenohpico.a` archive (avoids linker order issues)

**`packages/zpico/zpico-smoltcp/c/mbedtls_config.h`** (new):
- Minimal bare-metal config: TLS 1.2 client, X.509, SHA-256, AES-GCM/CBC, RSA, ECDHE
- `MBEDTLS_PLATFORM_CALLOC_MACRO`/`FREE_MACRO` → `z_bare_metal_calloc`/`z_bare_metal_free`
  (compile-time macros eliminate all `calloc`/`free` references)
- `MBEDTLS_NO_PLATFORM_ENTROPY` + `MBEDTLS_ENTROPY_HARDWARE_ALT` for custom entropy
- SSL buffers reduced to 4096 bytes

**Binary size:** ~85 KB text increase (within 100 KB budget)

### 53.13 — Bare-metal TLS platform symbols (`tls_bare_metal.c`) ✓

**`packages/zpico/zpico-smoltcp/c/tls_bare_metal.c`** (new, ~600 lines):

Implements 9 zenoh-pico TLS platform functions in C (not Rust — mbedTLS structs
are too complex for FFI bindings). Custom BIO callbacks route through SmoltcpBridge FFI:

- `_z_tls_bio_send_smoltcp` / `_z_tls_bio_recv_smoltcp` — use `smoltcp_socket_send`/`recv`
- Static TLS context pool (`ZPICO_SMOLTCP_MAX_TLS_SOCKETS`, default 1)
- Base64-only certificate loading (no filesystem)
- Handshake loop with `smoltcp_poll_network()` interleaving and 30s timeout

**`packages/zpico/zpico-smoltcp/c/entropy_bare_metal.c`** (new):
- Weak `mbedtls_hardware_poll()` using DWT cycle counter + splitmix32 mixing
- Platform crates with hardware RNG can override

**`packages/zpico/zpico-smoltcp/src/bridge.rs`:**
- Added `smoltcp_poll_network()` and `smoltcp_clock_ms()` FFI exports

### 53.14 — Bare-metal example: TLS on QEMU ARM ✓

Feature wiring through the crate chain:

- `examples/qemu-arm-baremetal/rust/zenoh/{talker,listener}/Cargo.toml` — added `link-tls` feature
- `packages/boards/nros-board-mps2-an385/Cargo.toml` — added `link-tls` feature forwarding
- `packages/zpico/zpico-platform-mps2-an385/Cargo.toml` — added `link-tls` feature
- `packages/zpico/zpico-platform-mps2-an385/src/memory.rs` — conditional heap:
  64 KB default, 128 KB with `link-tls`

**Binary sizes (QEMU ARM talker):**
- Without TLS: 113 KB text, 117 KB BSS
- With TLS: 198 KB text, 193 KB BSS (85 KB code increase, within 100 KB budget)

### 53.15 — TLS documentation ✓

- `docs/reference/environment-variables.md` — TLS notes section (POSIX vs bare-metal differences)
- `docs/guides/quick-reference.md` — TLS transport section with certificate generation,
  native and bare-metal usage instructions
- mbedTLS system dependency handled by `just setup` (already installs `libmbedtls-dev`)

### 53.16 — TLS integration test ✓

Added automated TLS integration test to the test framework.

**`packages/testing/nros-tests/src/fixtures/tls_certs.rs`** (new):
- `TlsCerts` struct: generates self-signed EC certificates via `openssl` CLI
- `is_openssl_available()` helper for skip logic
- Certificates use `CN=localhost` and prime256v1 curve

**`packages/testing/nros-tests/src/fixtures/zenohd_router.rs`:**
- `ZenohRouter::start_tls()` / `start_tls_unique()` — start zenohd with TLS listener
- `locator()` returns `tls/localhost:PORT` (not `127.0.0.1`) to match cert CN

**`packages/testing/nros-tests/src/fixtures/binaries.rs`:**
- `build_native_talker_tls()` / `build_native_listener_tls()` — build with `--features link-tls`
- Uses `--target-dir target-tls` for parallel build isolation
- `talker_tls_binary()` / `listener_tls_binary()` rstest fixtures

**`packages/testing/nros-tests/tests/nano2nano.rs`:**
- `test_tls_talker_listener_communication` — generates certs, starts TLS router,
  launches TLS talker/listener, verifies message delivery

**Key design choice:** TLS locator uses `localhost` (not `127.0.0.1`) because the
self-signed cert has `CN=localhost` and zenoh-pico's default `verify_name_on_connect=true`
requires hostname matching.

---

## Implementation Order

**UDP (complete):** 53.1 → 53.2 → 53.3 → 53.4 → 53.5 → 53.6 → 53.7

**TLS:** 53.8 → 53.9 → 53.10 → 53.11 → 53.12 → 53.13 → 53.14 → 53.15 → 53.16

53.8–53.10 (POSIX TLS) can proceed independently of 53.11–53.14 (bare-metal TLS).

## Key Files

### UDP (complete)

| File                                         | Change                                      |
|----------------------------------------------|---------------------------------------------|
| `packages/core/nros/Cargo.toml`              | Add `link-udp-unicast` feature              |
| `packages/zpico/nros-rmw-zenoh/Cargo.toml`   | Add `link-udp-unicast` feature              |
| `packages/zpico/zpico-smoltcp/Cargo.toml`    | Add `socket-udp`, `link-udp-unicast`        |
| `packages/zpico/zpico-smoltcp/src/bridge.rs` | Separate UDP socket table + poll loop       |
| `packages/zpico/zpico-smoltcp/src/udp.rs`    | **New** — UDP platform symbols              |
| `packages/zpico/zpico-smoltcp/src/util.rs`   | **New** — shared parse helpers              |
| `packages/zpico/zpico-smoltcp/src/tcp.rs`    | Use shared util module                      |
| `packages/zpico/zpico-smoltcp/src/lib.rs`    | UDP buffer statics + registration           |
| `packages/zpico/zpico-smoltcp/build.rs`      | MAX_UDP_SOCKETS config                      |
| `packages/boards/nros-*/src/node.rs`         | UDP socket registration in `init_network()` |
| `packages/boards/nros-*/Cargo.toml`          | Add `socket-udp` to smoltcp features        |

### TLS (complete)

| File                                                              | Change                                            |
|-------------------------------------------------------------------|---------------------------------------------------|
| `packages/zpico/zpico-sys/Cargo.toml`                             | Add `link-tls` feature                            |
| `packages/zpico/zpico-sys/build.rs`                               | TLS feature flag, mbedTLS compile (POSIX + embed) |
| `packages/zpico/zpico-sys/mbedtls/`                               | **New** — git submodule (mbedTLS v2.28.9)         |
| `packages/zpico/nros-rmw-zenoh/Cargo.toml`                        | Add `link-tls` feature forwarding                 |
| `packages/zpico/nros-rmw-zenoh/src/shim.rs`                       | TLS env var mappings                              |
| `packages/core/nros/Cargo.toml`                                   | Add `link-tls` feature forwarding                 |
| `packages/zpico/zpico-sys/c/platform/zenoh_bare_metal_platform.h` | Add `_tls_sock` field + `_fd` union               |
| `packages/zpico/zpico-sys/c/shim/zenoh_shim.c`                    | TLS property key mappings                         |
| `packages/zpico/zpico-smoltcp/Cargo.toml`                         | Add `link-tls` feature                            |
| `packages/zpico/zpico-smoltcp/src/tcp.rs`                         | Add `_tls_sock` to `ZSysNetSocket`                |
| `packages/zpico/zpico-smoltcp/src/bridge.rs`                      | Add `smoltcp_poll_network` + `smoltcp_clock_ms`   |
| `packages/zpico/zpico-smoltcp/c/tls_bare_metal.c`                 | **New** — 9 TLS platform symbols + BIO callbacks  |
| `packages/zpico/zpico-smoltcp/c/entropy_bare_metal.c`             | **New** — DWT-based entropy source                |
| `packages/zpico/zpico-smoltcp/c/mbedtls_config.h`                 | **New** — bare-metal mbedTLS config               |
| `packages/boards/nros-board-mps2-an385/Cargo.toml`                      | Add `link-tls` feature forwarding                 |
| `packages/zpico/zpico-platform-mps2-an385/Cargo.toml`             | Add `link-tls` feature forwarding                 |
| `packages/zpico/zpico-platform-mps2-an385/src/memory.rs`          | Conditional heap (64KB / 128KB with TLS)          |
| `packages/testing/nros-tests/src/fixtures/tls_certs.rs`           | **New** — TLS cert generation for tests           |
| `packages/testing/nros-tests/src/fixtures/zenohd_router.rs`       | TLS router support                                |
| `packages/testing/nros-tests/src/fixtures/binaries.rs`            | TLS binary builders                               |
| `packages/testing/nros-tests/tests/nano2nano.rs`                  | TLS integration test                              |

## Verification

### UDP (complete)

1. ~~`just quality` passes~~
2. ~~Native talker/listener works with `ZENOH_LOCATOR=udp/127.0.0.1:7447`~~
3. ~~QEMU ARM example builds and runs with `link-udp-unicast`~~

### TLS (complete)

1. ~~`just quality` passes (with and without `link-tls`)~~
2. ~~Native talker/listener works with `ZENOH_LOCATOR=tls/localhost:7447`~~
3. ~~QEMU ARM example builds with `--features link-tls` (85 KB text increase)~~
4. ~~Binary size delta < 100 KB for bare-metal TLS~~
5. ~~No regressions when `link-tls` is not enabled~~
6. ~~Automated TLS integration test passes (`test_tls_talker_listener_communication`)~~
