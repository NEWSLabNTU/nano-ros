# Phase 40 — Large Message Support

## Status: In Progress (40.1–40.4 complete)

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
  → z_loaned_bytes_t (fragmented arc-slice vector)
  → z_bytes_to_slice [Copy 1: malloc+memcpy, zenoh_shim.c:194]
  → z_owned_slice_t (malloc'd contiguous buffer)
  → entry->callback_ext(data, len, ...) → Rust FFI
  → copy_nonoverlapping [Copy 2: shim.rs:1023]
  → SUBSCRIBER_BUFFERS[i].data (1024-byte static buffer)
  → z_slice_drop (frees Copy 1 malloc)
  ...
  → try_recv_raw [Copy 3: shim.rs:1286]
  → ConnectedSubscriber.rx_buffer (1024-byte user buffer)
  → CdrReader::deserialize → typed message M
```

### Bottleneck layers

| Layer                                        | Native (posix) | Embedded | File                             |
|----------------------------------------------|----------------|----------|----------------------------------|
| zenoh-pico defrag (`Z_FRAG_MAX_SIZE`)        | 65536¹         | 2048     | `zpico-sys/build.rs`             |
| zenoh-pico batch (`Z_BATCH_UNICAST_SIZE`)    | 65536¹         | 1024     | `zpico-sys/build.rs`             |
| Shim static buffer (`SubscriberBuffer.data`) | 1024²          | 1024²    | `nros-rmw-zenoh/src/shim.rs`     |
| User receive buffer (`RX_BUF`)               | 1024²          | 1024²    | `nros-node/src/connected.rs`     |

¹ Configurable via `ZPICO_FRAG_MAX_SIZE` / `ZPICO_BATCH_UNICAST_SIZE` env vars.
² Per-entity buffer sizes are named constants (`SUBSCRIBER_BUFFER_SIZE`,
`DEFAULT_RX_BUFFER_SIZE`). Users can increase the user receive buffer via
`create_subscriber_sized::<M, BUF_SIZE>()`.

### Fragmentation

Messages larger than `Z_BATCH_UNICAST_SIZE` are fragmented by zenoh-pico.
Reassembly overflow (payload > `Z_FRAG_MAX_SIZE`) is silently dropped by the
zenoh-pico defragmentation layer.

### Service buffer overflow

~~The `ServiceBuffer` had no overflow flag — the callback silently truncated
oversized requests.~~ **Fixed in Phase 40.1**: Both zenoh and XRCE service
buffers now have `overflow: AtomicBool` flags that are set when a request
exceeds the buffer capacity. `try_recv_request()` checks the flag and returns
`TransportError::MessageTooLarge` instead of silently delivering corrupted data.

## Current Architecture — XRCE-DDS Backend

### Receive path (1 app-level copy)

```
XRCE Agent → UDP transport (MTU-sized datagrams)
  → XRCE session reassembly (reliable stream, 4 × MTU)
  → topic_callback copy_from_slice [Copy: nros-rmw-xrce/src/lib.rs:395]
  → SUBSCRIBER_SLOTS[i].data (1024-byte static buffer)
  ...
  → try_recv_raw
  → user buffer
  → CdrReader::deserialize → typed message M
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

~~Both `request_callback` and `reply_callback` silently discarded oversized
messages with an early `return` — no error flag was set.~~ **Fixed in Phase
40.1**: Both callbacks now set `overflow: AtomicBool` when the message exceeds
`BUFFER_SIZE`, and `try_recv_request()` / `call_raw()` return
`TransportError::MessageTooLarge`.

## Cross-Backend Comparison

| Aspect                | Zenoh              | XRCE-DDS                |
|-----------------------|--------------------|-------------------------|
| Per-entity buffer     | 1024 B             | 1024 B                  |
| Transport limit       | 64 KB (posix) / 2 KB (embedded) | 4 KB (posix) / 512 B (embedded) |
| Fragmentation (TX)    | Yes (built-in)     | Yes (fast path + fragmented fallback) |
| Copies per receive    | 1 via executor (direct write + in-place) | 0 via executor (in-place) |
| Sub overflow signal   | Yes (flag → error) | Yes (flag → error)      |
| Svc overflow signal   | Yes (flag → error) | Yes (flag → error)      |
| Practical max publish | 64 KB (posix)¹     | ~16 KB (posix)² / ~2 KB (embedded)² |
| Practical max receive | 1024 B³            | 1024 B³                 |

¹ Limited by zenoh-pico defrag/batch. Configurable via env vars.
² With fragmented streams (40.3), limited by reliable stream buffer (4 × MTU).
³ Limited by per-entity static buffer (`SUBSCRIBER_BUFFER_SIZE` / `BUFFER_SIZE`).
User RX buffer can be increased via `create_subscriber_sized`, but the shim
buffer is the binding constraint.

## Issues

| ID  | Issue                                                             | Backends | Severity | Status      |
|-----|-------------------------------------------------------------------|----------|----------|-------------|
| I1  | Hardcoded 1 KB shim/entity buffers                                | Both     | Critical | Named consts (40.1) |
| I2  | Hardcoded 1 KB publish buffer in `ConnectedPublisher::publish()`  | Zenoh¹   | High     | Const generic (40.1) |
| I3  | Three copies per received message                                 | Zenoh    | High     | Fixed: 3→1 (40.4 A–D) |
| I4  | Silent truncation/discard on service buffers                      | Both     | High     | Fixed (40.1) |
| I5  | Silent drop on zenoh defrag overflow                              | Zenoh    | Medium   | Mitigated (40.2) |
| I6  | Fixed static buffer count (8 sub, 8 svc)                          | Both     | Medium   | Named consts (40.2) |
| I7  | `Z_FEATURE_LOCAL_SUBSCRIBER` disabled (no intra-process shortcut) | Zenoh    | Low      | Won't fix (evaluated 40.4E) |
| I8  | Embedded defrag limit too small (2 KB)                            | Zenoh    | Medium   | Configurable (40.2) |
| I9  | 512-byte XRCE transport MTU                                       | XRCE     | Critical | Configurable, 4096 posix (40.2) |
| I10 | XRCE fragmented streams not used                                  | XRCE     | High     | Fixed (40.3) |
| I11 | Callback overwrites buffer during reader copy (data race)          | Both     | High     | Fixed (40.4 Part A) |

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

## Phase 40.4 — Receive Path Optimization (3→1 zenoh, 1→0 XRCE)

Three tightly coupled optimizations delivered as a single phase:

1. **Buffer lock** — Add `locked: AtomicBool` to prevent concurrent
   callback writes during reader access. Fixes a latent data race (I11) and
   is a prerequisite for both the direct-write path and in-place processing.
   (Both backends)

2. **Eliminate zenoh malloc** — Replace `z_bytes_to_slice()` (malloc+memcpy)
   with `z_bytes_reader_read()` that writes directly into the static buffer.
   Merges Copy 1 + Copy 2 into one. (Zenoh-only)

3. **In-place processing** — Add `process_raw_in_place` to the `Subscriber`
   trait so the executor deserializes directly from the static buffer,
   eliminating Copy 3. (Both backends)

### Why combined

- The `locked` flag is required for the direct-write path to be sound —
  without it, `z_bytes_reader_read()` into the static buffer has the same
  data race as the current callback.
- Items 2 and 3 both modify the zenoh C shim callback, Rust shim callback,
  and `ShimSubscriber` impl. Doing them separately means touching the same
  code twice.
- The `locked` flag also retrofits into the existing `try_recv_raw` path,
  fixing I11 for all users.

### Copy reduction

| Path          | Before                       | After                                   |
|---------------|------------------------------|-----------------------------------------|
| Zenoh receive | 3 (malloc → static → rx_buf) | **1** (reader → static, deser in-place) |
| XRCE receive  | 1 (slot → user buf)          | **0** (deser in-place from slot)        |

### Current zenoh flow (3 copies)

```
z_loaned_bytes_t (zenoh internal, fragmented arc-slices)
  │
  ├── COPY 1: z_bytes_to_slice() ──→ malloc'd z_owned_slice_t
  │                                   zenoh_shim.c:194
  │
  ├── entry->callback_ext(data, len, ...)
  │
  ├── COPY 2: copy_nonoverlapping ──→ SUBSCRIBER_BUFFERS[i].data
  │                                   shim.rs:1023
  │
  ├── z_slice_drop() ──→ frees malloc'd buffer
  │
  └── ... user calls try_recv_raw() ...
        │
        └── COPY 3: copy_nonoverlapping ──→ user rx_buffer
                                            shim.rs:1286
```

### Target zenoh flow (1 copy)

```
z_loaned_bytes_t (zenoh internal, fragmented arc-slices)
  │
  ├── Check locked flag (if locked → drop message)
  │
  ├── COPY 1: z_bytes_reader_read() ──→ SUBSCRIBER_BUFFERS[i].data
  │            Reads directly from arc-slices into static buffer.
  │            No intermediate malloc. zenoh_shim.c (modified)
  │
  └── ... executor calls process_raw_in_place() ...
        │
        ├── locked.store(true)
        ├── CdrReader::deserialize(&buffer.data[..len]) ──→ typed M
        ├── user_callback(&msg)
        ├── locked.store(false)
        └── has_data.store(false)
```

### zenoh-pico bytes API selection

| API                                              | Stable       | Allocates | Copies      | Use Case                                          |
|--------------------------------------------------|--------------|-----------|-------------|---------------------------------------------------|
| `z_bytes_to_slice()`                             | Yes          | malloc    | 1 full      | Current: coalesces fragments into malloc'd buffer |
| `z_bytes_get_contiguous_view()`                  | **Unstable** | No        | 0           | Zero-copy view, fails if fragmented               |
| `z_bytes_get_reader()` + `z_bytes_reader_read()` | Yes          | No        | 1 (direct)  | Read fragments into caller's buffer               |
| `z_bytes_get_slice_iterator()`                   | Yes          | No        | 0 per slice | Iterate over raw fragment slices                  |

**Chosen**: `z_bytes_reader_read()` — stable, no allocation, reads directly
into the target buffer. Works for both contiguous and fragmented payloads.

**Why not `z_bytes_get_contiguous_view()`**: Unstable
(`Z_FEATURE_UNSTABLE_API`), fails for fragmented messages. Would need a
fallback path.

**Why not `z_bytes_get_slice_iterator()`**: Changes the FFI boundary and
`SubscriberBuffer` layout to accept scatter-gather — too invasive.

### Implementation

#### Part A — Buffer lock (both backends)

Add `locked: AtomicBool` to `SubscriberBuffer` (zenoh) and `SubscriberSlot`
(XRCE).

**Writer side** (transport callback):

```rust
// In subscriber_callback_with_attachment (shim.rs) / topic_callback (lib.rs):
if buffer.locked.load(Ordering::Acquire) {
    // Reader is processing — drop this message.
    // Same behavior as today's depth-1 last-write-wins.
    return;
}
// ... existing write path ...
```

**Reader side** (both `process_raw_in_place` and existing `try_recv_raw`):

```rust
buffer.locked.store(true, Ordering::Release);
// -- buffer contents stable --
f(&buffer.data[..len]);  // or: copy to user buf
buffer.locked.store(false, Ordering::Release);
buffer.has_data.store(false, Ordering::Release);
```

**Cost**: 1 byte per subscriber slot. Messages during the lock window are
dropped — same depth-1 last-write-wins semantics as today.

#### Part B — Zenoh C shim direct write

**C shim changes** (`zpico-sys/c/shim/zenoh_shim.c`):

Add `buf_ptr` + `buf_capacity` + `locked_ptr` to `subscriber_entry_t`.
Replace `shim_sample_handler` with a direct-write variant:

```c
static void shim_sample_handler(z_loaned_sample_t *sample, void *arg) {
    subscriber_entry_t *entry = &g_subscribers[idx];

    // Check lock (Rust reader is processing)
    if (__atomic_load_n(entry->locked_ptr, __ATOMIC_ACQUIRE)) {
        return;
    }

    const z_loaned_bytes_t *payload = z_sample_payload(sample);
    size_t payload_len = z_bytes_len(payload);

    if (payload_len > entry->buf_capacity) {
        entry->callback_overflow(entry->ctx);
        return;
    }

    // Read directly into Rust's static buffer — no malloc
    z_bytes_reader_t reader = z_bytes_get_reader(payload);
    z_bytes_reader_read(&reader, entry->buf_ptr, payload_len);

    // Attachment still uses z_bytes_to_slice (33-37 bytes, negligible)
    // ... extract attachment ...

    entry->callback_notify(payload_len, att_data, att_len, entry->ctx);
}
```

**Rust shim changes** (`nros-rmw-zenoh/src/shim.rs`):

1. During `create_subscriber()`, register `buf_ptr`, `buf_capacity`, and
   `locked_ptr` with the C shim entry — these point into
   `SUBSCRIBER_BUFFERS[i]`.
2. Replace `subscriber_callback_with_attachment(data, len, ...)` with
   `subscriber_callback_notify(len, att_data, att_len, ctx)` — payload is
   already in the static buffer, callback only stores length + attachment +
   sets `has_data`.

#### Part C — `process_raw_in_place` trait + impls

**`Subscriber` trait** (`nros-rmw/src/traits.rs`):

```rust
/// Process the received message in-place without copying.
///
/// Calls `f` with a reference to the raw CDR bytes in the internal buffer.
/// The buffer is locked during `f` — the transport callback drops any
/// messages that arrive while the closure executes.
///
/// Returns `Ok(true)` if a message was available and `f` was called.
fn process_raw_in_place(
    &mut self,
    f: impl FnOnce(&[u8]),
) -> Result<bool, Self::Error>;
```

`Subscriber` is never used as `dyn` — type erasure happens at the
`ErasedCallback` level — so `impl FnOnce` is safe here.

**Zenoh impl** (`ShimSubscriber`):

```rust
fn process_raw_in_place(
    &mut self,
    f: impl FnOnce(&[u8]),
) -> Result<bool, TransportError> {
    let buffer = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index] };

    if !buffer.has_data.load(Ordering::Acquire) {
        return Ok(false);
    }
    if buffer.overflow.load(Ordering::Acquire) {
        buffer.overflow.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);
        return Err(TransportError::MessageTooLarge);
    }

    let len = buffer.len.load(Ordering::Acquire);
    buffer.locked.store(true, Ordering::Release);

    f(unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index].data[..len] });

    buffer.locked.store(false, Ordering::Release);
    buffer.has_data.store(false, Ordering::Release);
    Ok(true)
}
```

Also: `process_raw_in_place_with_info` parses attachment inside lock window.

**XRCE impl** (`XrceSubscriber`): Same pattern on `SUBSCRIBER_SLOTS`.

#### Part D — Typed wrappers + executor

**`ConnectedSubscriber`** (`nros-node/src/connected.rs`):

```rust
pub fn process_in_place(
    &mut self,
    f: impl FnOnce(&M),
) -> Result<bool, ConnectedNodeError> {
    let mut deser_err = false;
    let processed = self.subscriber.process_raw_in_place(|raw| {
        match CdrReader::new_with_header(raw)
            .and_then(|mut r| M::deserialize(&mut r))
        {
            Ok(msg) => f(&msg),
            Err(_) => deser_err = true,
        }
    }).map_err(|_| ConnectedNodeError::TransportError)?;

    if deser_err {
        return Err(ConnectedNodeError::DeserializationFailed);
    }
    Ok(processed)
}
```

`RX_BUF` is unused by this method. Existing `try_recv()` remains for the
copy-based path. Variant: `process_in_place_with_info` (zenoh).

**Executor** (`nros-node/src/executor.rs`):

`ErasedCallback::try_process()` uses in-place with split borrows:

```rust
fn try_process(&mut self) -> Result<bool, RclrsError> {
    let callback = &mut self.callback;
    self.subscriber.process_in_place(|msg| {
        callback.call(msg);
    }).map_err(|_| RclrsError::DeserializationFailed)
}
```

`SubscriptionEntryWithInfo` and `SubscriptionEntryWithSafety` follow the same
pattern. No changes to the `ErasedCallback` trait itself.

**XRCE node API** (`nros-node/src/xrce.rs`):

```rust
impl<M: RosMessage + Deserialize> XrceNodeSubscription<M> {
    pub fn process_in_place(
        &mut self,
        f: impl FnOnce(&M),
    ) -> Result<bool, XrceNodeError> {
        let mut deser_err = false;
        let processed = self.inner.process_raw_in_place(|raw| {
            match CdrReader::new_with_header(raw)
                .and_then(|mut r| M::deserialize(&mut r))
            {
                Ok(msg) => f(&msg),
                Err(_) => deser_err = true,
            }
        }).map_err(XrceNodeError::Transport)?;

        if deser_err {
            return Err(XrceNodeError::Deserialization);
        }
        Ok(processed)
    }
}
```

### `no_std` compatibility

Fully `no_std` compatible:
- `AtomicBool` is `core`
- `impl FnOnce(&[u8])` monomorphized at compile time (no allocation)
- Function pointers (`fn(&M)`) work without `alloc`

### Interaction with large messages

The static buffer is already the binding constraint on receive size. Messages
exceeding it are rejected as `MessageTooLarge` regardless of API. To receive
larger messages, increase the static buffer constant (40.1/40.2). The user's
`RX_BUF` const generic becomes irrelevant for the in-place path.

### Backward compatibility

- `try_recv()` / `try_recv_raw(buf)` remain unchanged (copy-based path)
- `ConnectedSubscriber<M, RX_BUF>` keeps `rx_buffer` for the copy-based path
- Executor transparently uses in-place (no user API change)
- XRCE users get `process_in_place` as an opt-in alternative to `try_recv`
- The `locked` flag also retrofits into `try_recv_raw` to fix the latent race

### Risks

- **Buffer address stability** (zenoh C shim): `SUBSCRIBER_BUFFERS` is a
  module-level `static mut` — fixed address, can't move. The pointer
  registered with the C shim is always valid.
- **Attachment handling**: Attachments still use `z_bytes_to_slice()` in the
  C shim. They're 33-37 bytes, so the malloc overhead is negligible. Can
  optimize in a follow-up.
- **Lock window duration**: Deserialization runs under the lock. Complex
  messages (large strings, sequences) take longer, widening the drop window.
  For robotics payloads at typical rates (10-100 Hz), this is not a concern.
  For high-rate telemetry (>1 kHz), the depth-1 semantics already drop
  messages anyway.

### Z_FEATURE_LOCAL_SUBSCRIBER evaluation (I7)

Currently disabled (`= 0`) in `zpico-sys/build.rs:206`. Assessment:

- **Not needed**: All nano-ros platforms use an external zenohd router for
  message routing. Local subscribers are only useful for intra-process pub/sub
  (same process publishes and subscribes on the same topic).
- **Adds complexity**: Enabling it changes the zenoh-pico session thread model
  and callback scheduling — the session thread would invoke local subscriber
  callbacks directly, bypassing the network path.
- **Different buffer model needed**: Zero-copy intra-process communication
  would require passing data between publisher and subscriber without going
  through the static buffer at all — a completely different architecture from
  the current callback-writes-to-static-buffer model.
- **Recommendation**: Keep disabled. Revisit only if intra-process latency
  becomes a measured bottleneck (unlikely for robotics use cases where nodes
  are separate processes).

### Performance notes

The copy reduction in 40.4 is structural (fewer `memcpy` calls in the data
path) rather than algorithmic. Estimated savings per received message:

- **1 KB payload**: ~1 us savings (2 fewer memcpy of 1 KB each)
- **64 KB payload**: ~10-20 us savings (2 fewer memcpy of 64 KB each)

These estimates assume modern x86 memory bandwidth (~40 GB/s). On embedded
targets (Cortex-M at 64 MHz), the savings are proportionally larger relative
to the total message processing time.

Formal benchmarking with criterion is deferred to a dedicated performance
phase. The existing `nano2nano` integration tests validate correctness through
the in-place executor path. The `sub_buf_in_place_matches_copy` unit test
confirms byte-level equivalence between the copy and in-place paths.

### Tasks

**Part A — Buffer lock (both backends)**
- [x] Add `locked: AtomicBool` to zenoh `SubscriberBuffer`
- [x] Add `locked: AtomicBool` to XRCE `SubscriberSlot`
- [x] Update zenoh `subscriber_callback_with_attachment` to check `locked`
- [x] Update XRCE `topic_callback` to check `locked`
- [x] Retrofit `locked` into existing `try_recv_raw` (both backends)

**Part B — Zenoh C shim direct write**
- [x] Add `buf_ptr` + `buf_capacity` + `locked_ptr` to `subscriber_entry_t`
- [x] Replace `z_bytes_to_slice` with `z_bytes_reader_read` in C shim
- [x] Replace Rust `subscriber_callback_with_attachment` with notify-only callback
- [x] Wire `ShimSubscriber::new()` to register buffer address with C shim

**Part C — In-place trait + impls**
- [x] Add `process_raw_in_place` to `Subscriber` trait
- [x] Implement for `ShimSubscriber` (zenoh)
- [x] Implement for `XrceSubscriber` (XRCE)
- [x] Add `process_raw_in_place_with_info` to `ShimSubscriber`

**Part D — Typed wrappers + executor**
- [x] Add `ConnectedSubscriber::process_in_place`
- [x] Add `ConnectedSubscriber::process_in_place_with_info`
- [x] Add `XrceNodeSubscription::process_in_place`
- [x] Update `ErasedCallback::try_process` impls to use in-place path

**Part E — Verification + testing**
- [x] Update ghost types: add `locked: bool` to `SubscriberBufferGhost`
- [x] Test: in-place deserialization matches copy-based path
- [x] Test: message dropped (not corrupted) when lock is held
- [x] Benchmark 1-copy vs 3-copy zenoh path (deferred to dedicated perf phase — see notes)
- [x] Evaluate enabling `Z_FEATURE_LOCAL_SUBSCRIBER` for intra-process (I7 — won't fix)
- [x] Fix Verus proofs: add `locked` field to all `SubscriberBufferGhost` literals
- [x] Add Verus proofs 13–15: lock correctness, process-in-place, data race prevention

## Phase 40.5 — Zero-Copy Receive (Future)

Explore eliminating the remaining copy (transport → static buffer) so user
code deserializes directly from the transport's internal buffers.

### Concept

With 40.4, the receive path is: 1 copy (zenoh → static buffer) + in-place
deserialization. The remaining copy is the write from the transport callback
into the static buffer. Eliminating it requires user code to run inside the
transport callback scope, before the transport drops the message.

Two possible approaches:

**A) Scatter-gather deserialization**: Instead of coalescing zenoh's fragmented
arc-slices into a contiguous buffer, pass the fragment list to a `CdrReader`
that chains reads across slices. This requires `z_bytes_get_slice_iterator()`
from zenoh-pico and changes the FFI boundary.

**B) Contiguous view for small messages**: Use
`z_bytes_get_contiguous_view()` for non-fragmented messages (zero-copy borrow)
and fall back to the reader path for fragmented messages. Requires
`Z_FEATURE_UNSTABLE_API`.

### Design constraints

- User code would run on the zenoh-pico session thread, blocking network I/O
- `z_loaned_sample_t` is stack-allocated by zenoh-pico's recv loop — cannot
  extend its lifetime
- Violates RTOS real-time scheduling expectations (user callback blocks
  transport)
- XRCE equivalent: `ucdrBuffer.iterator` points into the session's stream
  buffer and is only valid during the callback — same lifetime constraint

### Open questions

- [ ] Evaluate scatter-gather `CdrReader` (chains reads across fragment slices)
- [ ] Benchmark contiguous_view fallback path for fragmented messages
- [ ] Determine if running user code on transport thread is acceptable
- [ ] Assess `no_std` implications (callback from interrupt context on embedded)

## Testing

### XRCE Large Message Test

**Binary**: `examples/native/rust/xrce/large-msg-test/`
**Test**: `nros-tests::xrce::test_xrce_large_message_publish`

Publishes raw byte payloads at 9 sizes (64 B, 512 B, 1 KB, 2 KB, 3 KB, 4 KB,
6 KB, 8 KB, 12 KB) through an XRCE Agent. Messages above ~4 KB exceed a single
reliable stream slot and exercise the fragmented output stream path (`Phase
40.3`). All sizes pass on posix (MTU=4096, stream buffer=16384).

Run with:
```bash
just test-xrce                                      # All XRCE tests
cargo nextest run -p nros-tests -E 'test(xrce_large_message)'  # Just this test
```

### Zenoh Large Message Test (TODO)

No dedicated large-message integration test exists for the zenoh backend yet.
The existing `nano2nano.rs` tests use Int32 messages (8 bytes).

**What to test**: Publish and receive messages larger than `Z_BATCH_UNICAST_SIZE`
(1024 bytes on embedded, 65536 bytes on posix) to verify zenoh-pico
fragmentation and reassembly work end-to-end. The binding constraint is
`SUBSCRIBER_BUFFER_SIZE` (1024 bytes) on the receive side — messages larger than
this trigger `TransportError::MessageTooLarge`.

**Possible approaches**:

1. **Publish-only test** (similar to XRCE large-msg-test): Create a zenoh
   talker binary that publishes `String` messages of increasing sizes via
   `publish_raw()`. Verify the publish path succeeds for payloads up to the
   defrag limit. This tests zenoh-pico fragmentation on the TX side without
   requiring the receive-side buffer to match.

2. **End-to-end test with sized subscriber**: Create a talker/listener pair
   where the listener uses `create_subscriber_sized::<String, 8192>()` and
   `SUBSCRIBER_BUFFER_SIZE` is also increased (requires either changing the
   constant or adding build-time configurability to the shim buffer size).
   This tests the full path including deserialization.

3. **Overflow detection test**: Publish a message larger than
   `SUBSCRIBER_BUFFER_SIZE` and verify the listener reports
   `TransportError::MessageTooLarge` (not silent truncation). This validates
   Phase 40.1's overflow flag without needing buffer size changes.

**Recommended first step**: Approach 3 (overflow detection) validates the most
critical behavior (no silent corruption) and requires no buffer size changes.
Approach 1 can be added alongside it. Approach 2 is blocked until shim buffer
sizes become build-time configurable.

## Verification Requirements

- Verus proofs for `SubscriberBuffer` / `ServiceBuffer` state machines need
  parameterizing for configurable buffer sizes (currently hardcode capacity = 1024)
- Kani harnesses need updated size parameters to cover non-default buffer sizes
- Phase 37.1a buffer state machine tests (`ghost_capacity_constant`) must hold
  across all buffer sizes
- New overflow error paths (service buffer overflow flag) need proof coverage

## Key Files

| File                                           | Role                                                                |
|------------------------------------------------|---------------------------------------------------------------------|
| `packages/zpico/nros-rmw-zenoh/src/shim.rs`    | Zenoh shim buffers, subscriber/service callbacks, `locked` flag     |
| `packages/zpico/zpico-sys/c/shim/zenoh_shim.c` | C shim (`z_bytes_reader_read` direct write, lock check)             |
| `packages/zpico/zpico-sys/build.rs`            | Zenoh-pico build config (`Z_FRAG_MAX_SIZE`, `Z_BATCH_UNICAST_SIZE`) |
| `packages/xrce/nros-rmw-xrce/src/lib.rs`       | XRCE entity buffers, topic callback, `locked` flag, in-place impl  |
| `packages/xrce/xrce-sys/src/lib.rs`            | XRCE FFI (fragmented stream + flush callback declarations)          |
| `packages/xrce/xrce-sys/build.rs`              | XRCE `config.h` generation (`UXR_CONFIG_CUSTOM_TRANSPORT_MTU`)      |
| `packages/xrce/xrce-smoltcp/src/lib.rs`        | XRCE UDP staging buffers                                            |
| `packages/core/nros-rmw/src/traits.rs`         | `Subscriber` trait: `process_raw_in_place`                          |
| `packages/core/nros-node/src/connected.rs`     | `ConnectedSubscriber::process_in_place` (`#[cfg(rmw-zenoh)]`)      |
| `packages/core/nros-node/src/xrce.rs`          | `XrceNodeSubscription::process_in_place` (`#[cfg(rmw-xrce)]`)      |
| `packages/core/nros-node/src/executor.rs`      | `ErasedCallback::try_process` (uses in-place path)                  |
| `packages/core/nros/src/lib.rs`                | Unified crate: feature gates, mutual exclusivity checks             |
| `packages/core/nros-c/src/lib.rs`              | C API: all modules gated behind `#[cfg(feature = "rmw-zenoh")]`    |
| `packages/testing/nros-tests/tests/xrce.rs`    | XRCE integration tests (incl. large message publish test)           |
| `packages/testing/nros-tests/tests/nano2nano.rs` | Zenoh native pub/sub integration tests                            |
| `examples/native/rust/xrce/large-msg-test/`    | XRCE large message test binary (9 sizes, up to 12 KB)              |
