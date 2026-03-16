# Known Issues

Documented bugs, hardcoded values, and improvement opportunities.
Items here are candidates for future roadmap phases.

## ~~1. Hardcoded network configuration in board crates and examples~~ (Fixed)

Resolved by Phase 72: all examples now use `Config::from_toml(include_str!("../config.toml"))`
with per-example configuration files. Users change `config.toml` and rebuild —
no source code edits needed.

Board crate `Config::default()` / `Config::listener()` presets remain for
backwards compatibility but are no longer used by examples.

## ~~2. Zenoh-pico free list allocator on bare-metal~~ (Fixed)

All four bare-metal platform crates now share a single free-list allocator
via the `zpico-alloc` crate (`packages/zpico/zpico-alloc/`). This replaced
the broken bump allocators on ESP32/ESP32-QEMU/STM32F4 (which had no-op
`z_free` and data-losing `z_realloc`) with the proven MPS2-AN385 first-fit
free-list with address-ordered coalescing.

Each platform's `memory.rs` is now a thin wrapper that instantiates
`FreeListHeap<N>` with its heap size (32-128 KB).

**Remaining considerations** (not bugs):
- Fixed heap size — can't grow at runtime (inherent to bare-metal)
- First-fit fragmentation risk over very long sessions (hours+)
- `zpico-alloc` has an optional `stats` feature for heap usage tracking

## ~~3. Non-configurable compile-time constants~~ (Fixed)

Three user-facing constants are now configurable via environment variables:

| Env var                          | Default     | Constant                     | Crate          |
|----------------------------------|-------------|------------------------------|----------------|
| `NROS_SERVICE_TIMEOUT_MS`        | 10,000 ms   | `SERVICE_DEFAULT_TIMEOUT_MS` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `PARAM_SERVICE_BUFFER_SIZE`  | nros-node      |
| `NROS_KEYEXPR_STRING_SIZE`       | 256         | `KEYEXPR_STRING_SIZE`        | nros-rmw-zenoh |

`DEFAULT_MAX_TIMERS` was removed (dead code — timer count bounded by `MAX_CBS`).

Six internal constants remain intentionally non-configurable (safe defaults,
protocol-tied values).

## ~~4. `nano_ros_generate_interfaces()` requires explicit file listing~~ (Fixed)

Both the native and Zephyr CMake functions now support auto-discovery when
no files are specified. The C codegen also handles intra-package nested type
dependencies correctly (fully qualified type names, per-type `#include`
directives).

Cross-package dependencies must be declared with `DEPENDENCIES` and generated
separately.

## ~~5. Hardcoded opaque type sizes in nros-c and nros-cpp~~ (Fixed)

Opaque storage sizes for RMW handles are now computed from
`core::mem::size_of` at compile time — they always match the actual Rust
type layout and auto-adjust when types change. No manual maintenance needed.

- **nros-c**: `opaque_sizes.rs` computes sizes from `size_of::<RmwSession>()` etc.
- **nros-cpp**: `lib.rs` computes sizes from `size_of::<CppPublisher>()` etc.

## 6. Two separate heap allocators on RTOS platforms

On RTOS platforms (FreeRTOS, ThreadX), there are **two independent heap
allocators** that cannot share memory or statistics:

| Allocator                      | Who calls it                                                              | Backed by                                                                      |
|--------------------------------|---------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| zenoh-pico `z_malloc`/`z_free` | zenoh-pico C code (session state, buffers, hashmap buckets, vec growth)   | RTOS allocator (e.g. `pvPortMalloc`, `tx_byte_allocate`)                       |
| Rust `#[global_allocator]`     | nros Rust crates when `alloc` feature is enabled (`Box`, `Vec`, `String`) | RTOS allocator on FreeRTOS (via `FreeRtosAllocator`); not available on ThreadX |

**Current state by platform**:

| Platform   | z_malloc backend                                                   | Rust global_allocator                                         | nros alloc feature   |
|------------|--------------------------------------------------------------------|---------------------------------------------------------------|----------------------|
| Bare-metal | `zpico-alloc` (static free-list, 32–128 KB)                        | None                                                          | Disabled             |
| FreeRTOS   | `pvPortMalloc` (C, in zenoh-pico `system/freertos/system.c`)       | `FreeRtosAllocator` → `pvPortMalloc` (in `nros-c/src/lib.rs`) | Disabled in examples |
| ThreadX    | `tx_byte_allocate` (C, in `zpico-sys/c/platform/threadx/system.c`) | None available                                                | Disabled             |
| NuttX      | libc `malloc` (C, via POSIX `system/unix/system.c`)                | Standard Rust allocator (libc `malloc`)                       | Enabled (`std`)      |
| Zephyr     | `k_malloc` (C, in zenoh-pico `system/zephyr/system.c`)             | Zephyr allocator (when configured)                            | Varies               |

**Concerns**:

1. **FreeRTOS `z_realloc` returns NULL** — zenoh-pico's FreeRTOS `system.c`
   does not implement `z_realloc` (returns NULL). If zenoh-pico ever calls
   `z_realloc` on a FreeRTOS target, the allocation silently fails.
   Current usage does not hit this path, but it's fragile.

2. **ThreadX has no Rust global allocator** — if a future nros feature
   requires `alloc` on ThreadX, there's no allocator bridge. FreeRTOS has
   `FreeRtosAllocator` in `nros-c/src/lib.rs`; ThreadX does not.

3. **Heap budgeting is split** — on FreeRTOS, both zenoh-pico (via
   `pvPortMalloc`) and Rust (via `FreeRtosAllocator` → `pvPortMalloc`)
   draw from the same FreeRTOS heap, but there's no visibility into how
   much each consumer uses. On bare-metal, zenoh-pico uses its own
   `zpico-alloc` heap while nros Rust code uses no heap at all.

4. **Bare-metal could unify** — the `zpico-alloc` free-list heap could
   also serve as a Rust `#[global_allocator]` (implement `GlobalAlloc`
   for `FreeListHeap`), giving bare-metal targets a single heap for
   both C and Rust allocations. This is what the DDS backend already
   does (Phase 70).

**Possible improvements**:

- Implement `z_realloc` for FreeRTOS (alloc + memcpy + free, same as
  the ThreadX implementation already does).
- Add a `ThreadXAllocator` implementing `GlobalAlloc` via
  `tx_byte_allocate`/`tx_byte_release` for future `alloc` support.
- Implement `GlobalAlloc` on `FreeListHeap` so bare-metal platforms
  can optionally use `zpico-alloc` as the Rust global allocator too,
  creating a single unified heap.
- Add heap usage tracking (`stats` feature on `zpico-alloc`) to RTOS
  platforms as well, so developers can monitor total heap pressure.

## 7. Unbounded message sequences waste memory or cannot hold large payloads

Generated message bindings use `heapless::Vec<T, N>` for unbounded sequences
(`uint8[] data`, `float32[] ranges`, etc.). The capacity `N` is hardcoded in
the codegen at **64 elements** (`NROS_DEFAULT_SEQUENCE_CAPACITY` in
`packages/codegen/packages/rosidl-codegen/src/types.rs`).

This creates a fundamental mismatch for messages with large variable-length
payloads:

| Message                   | Field              | Typical size          | Generated capacity |
|---------------------------|--------------------|-----------------------|--------------------|
| `sensor_msgs/Image`       | `uint8[] data`     | 921,600 (640×480 RGB) | 64 bytes           |
| `sensor_msgs/PointCloud2` | `uint8[] data`     | 10,000+               | 64 bytes           |
| `sensor_msgs/LaserScan`   | `float32[] ranges` | 360–1080              | 64 floats          |
| `nav_msgs/OccupancyGrid`  | `int8[] data`      | 10,000+               | 64 bytes           |

**Problem**: `heapless::Vec<u8, 65536>` would support 64 KB images, but the
backing `[MaybeUninit<u8>; 65536]` **always occupies 64 KB** on the stack
regardless of actual content. On MCUs with 64–256 KB total RAM, this is
unacceptable.

Bounded sequences (`uint8[<=100] data`) use the specified max and do not
suffer from the default-capacity problem.

**Impact**: Large sensor messages (Image, PointCloud2, LaserScan) are
effectively unusable on embedded targets with the current codegen.
Deserialization fails with `DeserError::CapacityExceeded` when the incoming
data exceeds 64 elements.

**Design direction — borrowed deserialization (zero-copy)**:

Instead of copying sequence data into the message struct, generate a
borrowed message type where unbounded sequences are `&'a [u8]` slices
pointing directly into the CDR receive buffer:

```rust
// Current: copies data into fixed inline buffer (64 bytes max)
struct Image {
    height: u32,
    width: u32,
    encoding: heapless::String<32>,
    data: heapless::Vec<u8, 64>,  // 64 bytes on stack, always
}

// Proposed: borrows data from transport buffer (16 bytes on stack)
struct Image<'a> {
    height: u32,
    width: u32,
    encoding: heapless::String<32>,
    data: &'a [u8],  // pointer + length, points into CDR buffer
}
```

The deserializer reads the CDR sequence length header, then returns a slice
into the receive buffer at the correct offset — no copy, no fixed capacity.
The message struct is small and fixed-size. The payload can be arbitrarily
large, bounded only by the transport buffer size (`NROS_SUBSCRIPTION_BUFFER_SIZE`).

This works for any sequence field, not just the last one — the CDR
deserializer knows each field's offset. The lifetime `'a` ties the message
to the receive buffer scope (valid for the duration of the subscription
callback).

**Implementation approach**:

1. Add a `borrowed` codegen mode alongside the current `owned` mode.
   `owned` generates `heapless::Vec<T, N>` (current behavior, for small
   messages). `borrowed` generates `&'a [T]` for unbounded sequences.
2. The subscription callback receives `Image<'_>` with data borrowing
   the CDR buffer. The message is valid only inside the callback.
3. For non-byte sequences (`float32[] ranges`), alignment must be
   verified — CDR guarantees alignment, but the slice cast from
   `&[u8]` to `&[f32]` needs validation on strictly-aligned platforms.
4. Transport buffer size becomes the effective message size limit,
   configurable per-subscription via `NROS_SUBSCRIPTION_BUFFER_SIZE`.

**Workarounds available today**:

- Define bounded message types for the application's actual payload
  size (e.g., `uint8[<=4096] data` in a custom `.msg` file).
- Use raw CDR APIs (`try_recv_raw`) to access the receive buffer
  directly, bypassing the generated message types entirely.

## 8. Two-copy receive path and static buffer pre-allocation at scale

Every subscription message traverses two copies before reaching user code:

```
Network → SUBSCRIBER_BUFFERS[i].data → SubEntry.buffer (arena) → deserialize → callback
              (zenoh-pico direct write)     (memcpy in try_recv_raw)    (CDR field-by-field)
```

**Copy chain**:

| Copy | From                         | To                           | Location       | Method                               |
|------|------------------------------|------------------------------|----------------|--------------------------------------|
| —    | Network                      | `SUBSCRIBER_BUFFERS[i].data` | Static         | zenoh-pico direct write (no copy)    |
| #1   | `SUBSCRIBER_BUFFERS[i].data` | `SubEntry.buffer`            | Executor arena | `memcpy` in `try_recv_raw()`         |
| #2   | `SubEntry.buffer`            | Message struct               | Stack          | CDR deserialization (field-by-field) |

**Static memory pre-allocation** (default config):

| Buffer                 | Per-unit | Count                         | Default total |
|------------------------|----------|-------------------------------|---------------|
| `SUBSCRIBER_BUFFERS`   | ~1064 B  | `ZPICO_MAX_SUBSCRIBERS` (128) | **133 KB**    |
| Executor arena entries | ~2304 B  | `NROS_EXECUTOR_MAX_CBS` (4)   | **~10 KB**    |

The dominant cost is `SUBSCRIBER_BUFFERS`: 128 slots × buffer size, all
pre-allocated as a static array regardless of how many subscribers exist.

**Scaling problem**: If the buffer size is increased for large messages
(e.g., `ZPICO_SUBSCRIBER_BUFFER_SIZE=65536` for 64 KB compressed images),
the static array becomes 128 × 64 KB = **8 MB** — impossible on any MCU.
Reducing `ZPICO_MAX_SUBSCRIBERS` helps (e.g., 4 slots × 64 KB = 256 KB),
but then the system supports very few concurrent subscribers.

**CPU cost**: The two memcpy operations are negligible for small messages
(1 KB at 100 Hz = 200 KB/s). For large messages (64 KB at 30 Hz = 3.8 MB/s),
the copies are still feasible on Cortex-M4 @ 168 MHz but become a
meaningful fraction of available bandwidth.

**Design direction — single-copy alloc-free receive**:

The goal is to eliminate copy #1 (arena copy) so the user callback
deserializes directly from `SUBSCRIBER_BUFFERS`, reducing to one write
(network → static buffer) plus zero-copy deserialization:

```
Network → SUBSCRIBER_BUFFERS[i].data → borrowed deserialize → callback(&msg)
              (zenoh-pico direct write)    (slices into buffer, no copy)
```

This requires:

1. **Skip the arena buffer**: The executor dispatches directly from
   `SUBSCRIBER_BUFFERS` instead of copying into `SubEntry.buffer`.
   The subscriber buffer is locked (already has an atomic lock flag)
   during callback execution to prevent zenoh-pico from overwriting it.

2. **Borrowed deserialization** (issue 7): The message struct borrows
   `&'a [u8]` slices from the subscriber buffer for variable-length
   fields, avoiding the CDR copy into `heapless::Vec`.

3. **Reduce subscriber slot count**: Instead of 128 pre-allocated
   slots, size `ZPICO_MAX_SUBSCRIBERS` to the actual number of
   subscribers (e.g., 4–8). This is already configurable but defaults
   to 128.

Combined with issue 7's borrowed deserialization, this gives a
zero-copy path from network to user callback for the payload data,
with only fixed-size header fields deserialized onto the stack.

**Existing zero-copy path** (`unstable-zenoh-api`): Skips
`SUBSCRIBER_BUFFERS` entirely — the callback receives `&[u8]` pointing
into zenoh-pico's internal buffer. However, it requires `alloc`
(boxed callback closure) and bypasses the executor's LET semantics,
making it unsuitable for alloc-free bare-metal use.

**Workarounds available today**:

- Set `ZPICO_MAX_SUBSCRIBERS` to the actual subscriber count (e.g., 4)
  to reduce static memory waste.
- Increase `ZPICO_SUBSCRIBER_BUFFER_SIZE` only when large messages are
  needed, accepting the memory tradeoff.
- Use the raw CDR API (`try_recv_raw`) with a caller-provided buffer
  to bypass the static buffer system entirely.
