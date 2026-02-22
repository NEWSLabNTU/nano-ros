# Phase 53 — UDP Transport Support

## Context

nano-ros currently only supports TCP for the zenoh-pico transport layer. On native/POSIX, zenoh-pico already has full UDP support via OS sockets — it just isn't exposed. On bare-metal, only TCP is wired through the smoltcp bridge.

Principle: **use zenoh-pico's native UDP on POSIX, only use smoltcp on bare-metal**.

## Progress

| Item | Status |
|------|--------|
| 53.1 — Feature forwarding chain | Done |
| 53.2 — smoltcp UDP socket infrastructure | Done |
| 53.3 — UDP platform symbols (`udp.rs` + `util.rs`) | Done |
| 53.4 — Board crate UDP socket registration | Done |
| 53.5 — Native example: verify UDP locator | Not Started |
| 53.6 — Bare-metal example: UDP on QEMU ARM | Not Started |
| 53.7 — Documentation | Done |

## Deliverables

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

### 53.5 — Native example: verify UDP locator

No code changes. Verify that native talker/listener works with:
```
ZENOH_LOCATOR=udp/127.0.0.1:7447
```
Add a note to native example README documenting UDP usage.

### 53.6 — Bare-metal example: UDP on QEMU ARM

Add `link-udp-unicast` feature to a QEMU ARM example and test with `ZENOH_LOCATOR=udp/<bridge-ip>:7447`.

### 53.7 — Documentation ✓

- `docs/reference/environment-variables.md` — UDP locator format, `ZPICO_SMOLTCP_MAX_UDP_SOCKETS`
- `docs/guides/quick-reference.md` — UDP transport section

## Implementation Order

53.1 → 53.2 → 53.3 → 53.4 → 53.5 → 53.6 → 53.7

## Key Files

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

## Verification

1. `just quality` passes
2. Native talker/listener works with `ZENOH_LOCATOR=udp/127.0.0.1:7447`
3. QEMU ARM example builds and runs with `link-udp-unicast`
4. `just test-integration` passes
