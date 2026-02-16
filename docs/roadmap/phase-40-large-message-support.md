# Phase 40 — Large Message Support

## Status: In Progress (40.1 + 40.2 + 40.3 complete)

## Background

nano-ros aims to be a verifiable, safe ROS 2 client library suitable for all
platforms — not just embedded microcontrollers. Current buffer sizes are
hardcoded at 1 KB, which blocks use cases requiring larger payloads such as
camera frames (`sensor_msgs/Image`), point clouds (`sensor_msgs/PointCloud2`),
and occupancy grids (`nav_msgs/OccupancyGrid`).

Both RMW backends (zenoh-pico and XRCE-DDS) have different bottleneck profiles
but share the same fundamental limitation: fixed, small buffers with incomplete
overflow signalling.

Since the feature orthogonality refactor, neither backend is enabled by default
(`default = ["std"]` only). Users must explicitly select `rmw-zenoh` or
`rmw-xrce`, a platform, and a ROS edition. The two backends have completely
separate node APIs: `ConnectedNode`/`ConnectedPublisher` (zenoh, requires
`alloc`) vs `XrceExecutor`/`XrceNode` (XRCE). The C API (`nros-c`) is
currently zenoh-only — all modules are gated behind `#[cfg(feature =
"rmw-zenoh")]`.

This phase was identified during Phase 37.4 fairness benchmarking, where
throughput testing exposed silent truncation on service paths and practical
message size ceilings well below typical robotics payloads.

## Current Architecture — Zenoh Backend

### Receive path (3 copies)

```
zenoh-pico network → defrag (Z_FRAG_MAX_SIZE)
  → z_bytes_to_slice [Copy 1: malloc+memcpy, zenoh_shim.c:194]
  → Rust callback copy_nonoverlapping [Copy 2: shim.rs:1021]
  → try_recv_raw copy to user buffer [Copy 3: shim.rs:1147]
```

### Bottleneck layers

| Layer                                        | Native (posix) | Embedded | File                             |
|----------------------------------------------|----------------|----------|----------------------------------|
| zenoh-pico defrag (`Z_FRAG_MAX_SIZE`)        | 65536¹         | 2048     | `zpico-sys/build.rs`             |
| zenoh-pico batch (`Z_BATCH_UNICAST_SIZE`)    | 65536¹         | 1024     | `zpico-sys/build.rs`             |
| Shim static buffer (`SubscriberBuffer.data`) | 1024²          | 1024²    | `nros-rmw-zenoh/src/shim.rs`     |
| User receive buffer (`RX_BUF`)               | 1024²          | 1024²    | `nros-node/src/connected.rs`     |

¹ Configurable via `ZPICO_FRAG_MAX_SIZE` / `ZPICO_BATCH_UNICAST_SIZE` env vars.
² Per-entity buffer sizes are named constants; increasing them is a Phase 40.3+ concern.

### Fragmentation

Messages larger than `Z_BATCH_UNICAST_SIZE` (1024 bytes, `build.rs:81`) are
fragmented by zenoh-pico. Reassembly overflow (payload > `Z_FRAG_MAX_SIZE`) is
silently dropped by the zenoh-pico defragmentation layer.

### Service buffer overflow

~~The `ServiceBuffer` had no overflow flag — the callback silently truncated
oversized requests.~~ **Fixed in Phase 40.1**: Both zenoh and XRCE service
buffers now have `overflow: AtomicBool` flags that are set when a request
exceeds the buffer capacity. `try_recv_request()` checks the flag and returns
`TransportError::MessageTooLarge` instead of silently delivering corrupted data.

## Current Architecture — XRCE-DDS Backend

### Receive path (1 app-level copy)

```
XRCE Agent → UDP transport (512-byte MTU)
  → XRCE session parse
  → topic_callback copy_from_slice [Copy: nros-rmw-xrce/src/lib.rs:373]
```

### Bottleneck layers

| Layer                    | Native (posix) | Embedded     | File                          |
|--------------------------|----------------|--------------|-------------------------------|
| Transport MTU            | 4096¹          | 512¹         | `xrce-sys/build.rs`           |
| Stream buffer (reliable) | 16384 (4×4096) | 2048 (4×512) | `nros-rmw-xrce/src/lib.rs`    |
| Per-entity buffer        | 1024           | 1024         | `nros-rmw-xrce/src/lib.rs`    |
| UDP staging              | = MTU          | = MTU        | `xrce-smoltcp/src/lib.rs`     |

¹ Configurable via `XRCE_TRANSPORT_MTU` env var.

### Fragmentation

~~`uxr_prepare_output_stream_fragmented()` exists in the Micro-XRCE-DDS API but
is **not used** by nano-ros. All publishes use the non-fragmented path, limiting
effective payload to < MTU minus XRCE headers (~450-480 bytes).~~ **Fixed in
Phase 40.3**: `XrcePublisher::publish_raw()` now tries the non-fragmented fast
path first (`uxr_buffer_topic`), then falls back to
`uxr_prepare_output_stream_fragmented()` with a flush callback that flushes and
runs the session. This enables messages larger than a single stream slot.

### Service/client overflow

Both `request_callback` (`lib.rs:400`) and `reply_callback` (`lib.rs:432`)
silently discard oversized messages with an early `return` — no error flag is
set, and the application never learns a request was lost.

## Cross-Backend Comparison

| Aspect                | Zenoh              | XRCE-DDS                |
|-----------------------|--------------------|-------------------------|
| Per-entity buffer     | 1024 B             | 1024 B                  |
| Transport limit       | 64 KB (posix) / 2 KB (embedded) | 4 KB (posix) / 512 B (embedded) |
| Fragmentation used    | Yes (built-in)     | Yes (fast path + fragmented fallback) |
| Copies per receive    | 3                  | 1                       |
| Sub overflow signal   | Yes (flag → error) | Yes (flag → error)      |
| Svc overflow signal   | Yes (flag → error) | Yes (flag → error)      |
| Practical max message | ~1024 B¹           | ~16 KB (posix)² / ~2 KB (embedded)² |

¹ Still limited by per-entity shim buffer (1024 B), not by transport layer.
² With fragmented streams (40.3), limited by reliable stream buffer (4 × MTU).

## Issues

| ID  | Issue                                                             | Backends | Severity | Status      |
|-----|-------------------------------------------------------------------|----------|----------|-------------|
| I1  | Hardcoded 1 KB shim/entity buffers                                | Both     | Critical | Named consts (40.1) |
| I2  | Hardcoded 1 KB publish buffer in `ConnectedPublisher::publish()`  | Zenoh¹   | High     | Const generic (40.1) |
| I3  | Three copies per received message                                 | Zenoh    | High     | Open        |
| I4  | Silent truncation/discard on service buffers                      | Both     | High     | Fixed (40.1) |
| I5  | Silent drop on zenoh defrag overflow                              | Zenoh    | Medium   | Mitigated (40.2) |
| I6  | Fixed static buffer count (8 sub, 8 svc)                          | Both     | Medium   | Named consts (40.2) |
| I7  | `Z_FEATURE_LOCAL_SUBSCRIBER` disabled (no intra-process shortcut) | Zenoh    | Low      | Open        |
| I8  | Embedded defrag limit too small (2 KB)                            | Zenoh    | Medium   | Configurable (40.2) |
| I9  | 512-byte XRCE transport MTU                                       | XRCE     | Critical | Configurable, 4096 posix (40.2) |
| I10 | XRCE fragmented streams not used                                  | XRCE     | High     | Fixed (40.3) |

¹ The XRCE node API (`XrceNodePublisher::publish()`) already requires a
caller-supplied buffer, so I2 does not apply to XRCE.

## Phase 40.1 — Configurable Buffers (Quick Wins)

Make buffer sizes configurable without changing the static allocation model.

- [x] Make `SubscriberBuffer.data` size a const generic in zenoh shim (I1)
- [x] Make `BUFFER_SIZE` configurable in XRCE RMW (I1)
- [x] Add `overflow: bool` flag to zenoh `ServiceBuffer` (I4)
- [x] Add overflow flag to XRCE service server/client callbacks (I4)
- [x] Make `ConnectedPublisher::publish()` buffer size a const generic (I2)
- [x] Deprecate `publish_with_buffer()` workaround once generic publish lands (I2)

## Phase 40.2 — Platform-Appropriate Defaults

Set larger defaults for `platform-posix` builds while keeping
`platform-bare-metal` / `platform-zephyr` defaults small for memory-constrained
targets. Per the orthogonality principle, platform features must not imply an
RMW backend — defaults are scoped within each backend's build configuration.

- [x] Expose `Z_FRAG_MAX_SIZE` / `Z_BATCH_UNICAST_SIZE` as `build.rs` env vars (I5, I8)
- [x] Set `platform-posix` zenoh defrag default to 64 KB+ (I8)
- [x] Expose `UXR_CONFIG_CUSTOM_TRANSPORT_MTU` as configurable in `xrce-sys` (I9)
- [x] Increase `platform-posix` XRCE MTU default to 4096+ (I9)
- [x] Match `xrce-smoltcp` UDP staging buffers to new MTU (I9)
- [x] Make static buffer count configurable via const generic or feature (I6)

## Phase 40.3 — XRCE Fragmented Streams

Enable large message transport through the XRCE Agent using the existing
Micro-XRCE-DDS fragmentation API.

- [x] Implement `uxr_prepare_output_stream_fragmented()` support in publish path (I10)
- [x] Add flush callback for XRCE stream management (I10)
- [x] Test large message send/receive through XRCE Agent (I10)

## Phase 40.4 — Zenoh Receive Path Optimization

Reduce the copy count on the zenoh receive path from 3 to 2.

- [ ] Eliminate Copy 1: use `z_bytes_clone()` + direct arc_slice access in C shim (I3)
- [ ] Benchmark 2-copy vs 3-copy path for throughput improvement (I3)
- [ ] Evaluate enabling `Z_FEATURE_LOCAL_SUBSCRIBER` for intra-process optimization (I7)

## Phase 40.5 — Zero-Copy Receive (Future)

Explore a callback-based API where user code processes messages directly from
the transport buffer without intermediate copies.

- [ ] Design callback-based zero-copy API (user callback invoked from transport thread) (I3)
- [ ] Address lifetime and thread-safety requirements
- [ ] Prototype and benchmark against current buffered path

## Verification Requirements

- Verus proofs for `SubscriberBuffer` / `ServiceBuffer` state machines need
  parameterizing for configurable buffer sizes (currently hardcode capacity = 1024)
- Kani harnesses need updated size parameters to cover non-default buffer sizes
- Phase 37.1a buffer state machine tests (`ghost_capacity_constant`) must hold
  across all buffer sizes
- New overflow error paths (service buffer overflow flag) need proof coverage
- XRCE fragmentation needs integration test coverage via `nros-tests`

## Key Files

| File                                           | Role                                                                |
|------------------------------------------------|---------------------------------------------------------------------|
| `packages/zpico/nros-rmw-zenoh/src/shim.rs`    | Zenoh shim buffers, subscriber/service callbacks                    |
| `packages/zpico/zpico-sys/c/shim/zenoh_shim.c` | C shim (Copy 1: `z_bytes_to_slice`)                                 |
| `packages/zpico/zpico-sys/build.rs`            | Zenoh-pico build config (`Z_FRAG_MAX_SIZE`, `Z_BATCH_UNICAST_SIZE`) |
| `packages/xrce/nros-rmw-xrce/src/lib.rs`       | XRCE entity buffers, topic/service callbacks                        |
| `packages/xrce/xrce-sys/build.rs`              | XRCE `config.h` generation (`UXR_CONFIG_CUSTOM_TRANSPORT_MTU`)      |
| `packages/xrce/xrce-smoltcp/src/lib.rs`        | XRCE UDP staging buffers                                            |
| `packages/core/nros-node/src/connected.rs`     | Zenoh node API: `RX_BUF`, publish buffer (`#[cfg(rmw-zenoh)]`)     |
| `packages/core/nros-node/src/xrce.rs`          | XRCE node API: `XrceExecutor`, `XrceNode` (`#[cfg(rmw-xrce)]`)    |
| `packages/core/nros/src/lib.rs`                | Unified crate: feature gates, mutual exclusivity checks             |
| `packages/core/nros-c/src/lib.rs`              | C API: all modules gated behind `#[cfg(feature = "rmw-zenoh")]`    |
