# Phase 73 — Memory Efficiency and Zero-Copy Receive

**Goal**: Fix known memory bugs, eliminate unnecessary copies in the
subscription receive path, and enable large-message support on
memory-constrained embedded targets — all without requiring `alloc`.

**Status**: Not Started

**Priority**: High

**Depends on**: None (independent of Phase 70/71 DDS work)

## Overview

Three related issues (known-issues 6, 7, 8) converge on the same
problem: nano-ros's alloc-free design pre-allocates fixed-size buffers
and copies message data twice before it reaches user code. This works
for small messages on capable MCUs but breaks down for:

- **Large messages** (Image, PointCloud2, LaserScan) — the 64-element
  default `heapless::Vec` capacity is too small, but increasing it
  wastes memory because the backing array is always fully allocated.
- **High subscriber counts** — 128 pre-allocated subscriber buffers ×
  buffer size dominates RAM usage even when most slots are unused.
- **Real-time constraints** — zenoh-pico's C code calls `z_malloc`
  per string field per received message, and on FreeRTOS `z_realloc`
  returns NULL (latent bug).

### Current receive path (two copies)

```
Network → SUBSCRIBER_BUFFERS[i] → SubEntry.buffer (arena) → deserialize(owned) → callback
             (zenoh-pico write)      (memcpy #1)              (CDR parse #2)
```

### Target receive path (zero-copy)

```
Network → RMW buffer → lock → deserialize(borrowed) → callback(&MsgRef<'_>) → unlock
             (single write)    (slices into buffer, no copy)
```

## Architecture

### Borrowed message types

The codegen generates two variants per message:

```rust
// Owned — for publishing, storing beyond callback scope
pub struct Image {
    pub height: u32,
    pub width: u32,
    pub encoding: heapless::String<32>,
    pub data: heapless::Vec<u8, 64>,  // fixed capacity
}

// Borrowed — for subscription callbacks, zero-copy receive
pub struct ImageRef<'a> {
    pub height: u32,
    pub width: u32,
    pub encoding: &'a str,            // borrows from CDR buffer
    pub data: &'a [u8],              // borrows from CDR buffer
}
```

`ImageRef<'a>` is always small (fixed-size header fields + pointer/length
pairs). The payload data stays in the RMW receive buffer. The lifetime
`'a` ties the message to the callback scope.

The owned `Image` type remains for publishing and for cases where the
user needs to store a message beyond the callback.

### RMW buffer guard trait

A new method on the `Subscriber` trait in `nros-rmw` exposes the
receive buffer without copying:

```rust
// nros-rmw/src/traits.rs
pub trait Subscriber {
    // Existing: copies data out of the buffer
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ...>;

    // New: locks buffer and returns a reference (no copy)
    fn lock_recv_buffer(&mut self) -> Result<Option<RecvGuard<'_>>, ...>;
}

pub struct RecvGuard<'a> {
    data: &'a [u8],
    unlock: &'a AtomicBool,  // set to false on drop
}

impl Drop for RecvGuard<'_> {
    fn drop(&mut self) {
        self.unlock.store(false, Ordering::Release);
    }
}
```

This is RMW-agnostic: zenoh-pico, DDS, and XRCE-DDS backends all have
internal receive buffers that can be exposed through this pattern.
The lock is an atomic flag (not a mutex) — if a new message arrives
while locked, it is dropped. This is correct for sensor data where
only the latest reading matters (ROS 2 default `KEEP_LAST(1)` QoS).

Services and actions continue to use the copy-based `try_recv_raw`
path, where every message matters.

### Executor integration

The executor's `spin_once` dispatch gains a zero-copy path:

```rust
// Current: copy then deserialize
let len = entry.handle.try_recv_raw(&mut entry.buffer)?;
let msg: M = deserialize(&entry.buffer[..len]);
(entry.callback)(&msg);

// New: lock then borrow-deserialize
if let Some(guard) = entry.handle.lock_recv_buffer()? {
    let msg: MRef<'_> = deserialize_borrowed(guard.data);
    (entry.callback)(&msg);
    // guard dropped here → buffer unlocked
}
```

The `SubEntry` no longer needs its own `buffer: [u8; RX_BUF]` field
for the zero-copy path, shrinking the executor arena. The existing
copy-based path remains available for subscribers that opt out.

### Allocator improvements

The `zpico-alloc` free-list allocator is extended with a slab fast-path
for zenoh-pico's common allocation pattern (short-lived string fields
during message parsing):

```
z_malloc(48) → check 64-byte slab cache (O(1)) → hit: return slab slot
                                                → miss: fall through to free-list (O(n))
z_free(ptr)  → is ptr in slab region? → yes: return to slab (O(1))
                                      → no: free-list coalesce (O(n))
```

This makes the common case (string field alloc/free per message)
deterministic O(1) without any zenoh-pico patches.

## Work Items

- [ ] 73.1 — Fix FreeRTOS `z_realloc` (returns NULL)
- [ ] 73.2 — Fix ThreadX missing Rust `GlobalAlloc`
- [ ] 73.3 — Slab fast-path in `zpico-alloc`
- [ ] 73.4 — `RecvGuard` trait and zenoh-pico backend implementation
- [ ] 73.5 — Borrowed message codegen (`MsgRef<'a>` types)
- [ ] 73.6 — Executor zero-copy dispatch path
- [ ] 73.7 — DDS and XRCE-DDS `RecvGuard` implementations
- [ ] 73.8 — Reduce default `ZPICO_MAX_SUBSCRIBERS` and document sizing

### 73.1 — Fix FreeRTOS `z_realloc` (returns NULL)

zenoh-pico's `src/system/freertos/system.c` implements `z_realloc` as
`return NULL`. If zenoh-pico ever calls `z_realloc` on FreeRTOS, the
allocation silently fails. Current usage doesn't hit this path, but it
is fragile.

**Fix**: Implement `z_realloc` as alloc-copy-free, matching what the
ThreadX platform already does:

```c
void *z_realloc(void *ptr, size_t size) {
    if (ptr == NULL) return z_malloc(size);
    if (size == 0) { z_free(ptr); return NULL; }
    void *new_ptr = z_malloc(size);
    if (new_ptr == NULL) return NULL;
    // Copy min(old_size, new_size) — FreeRTOS doesn't expose block size,
    // so copy `size` bytes (safe: old block is at least old_size).
    memcpy(new_ptr, ptr, size);
    z_free(ptr);
    return new_ptr;
}
```

Note: FreeRTOS `pvPortMalloc` does not expose the allocated block size.
The `memcpy` uses `size` (new size) which may over-read if shrinking.
For safety, use the heap_4/heap_5 internal block header to read the
actual size (implementation-specific), or always copy `size` bytes and
accept the minor over-read (the old block is at least `old_size` ≥ some
reasonable minimum).

**Files**:
- `packages/zpico/zpico-sys/zenoh-pico/src/system/freertos/system.c`

### 73.2 — Fix ThreadX missing Rust `GlobalAlloc`

ThreadX provides `tx_byte_allocate`/`tx_byte_release` for C-level
allocation (used by zenoh-pico's `z_malloc`), but there is no Rust
`GlobalAlloc` implementation. If a future nros feature requires `alloc`
on ThreadX (e.g., parameter services), there's no allocator.

FreeRTOS already has `FreeRtosAllocator` in `nros-c/src/lib.rs`.
Add an equivalent `ThreadXAllocator`.

**Files**:
- `packages/core/nros-c/src/lib.rs` (add ThreadX allocator module)
- `packages/zpico/zpico-sys/c/platform/threadx/system.c` (reference for API)

### 73.3 — Slab fast-path in `zpico-alloc`

zenoh-pico allocates and frees short-lived string buffers per message
field during CDR parsing. The free-list handles this but with O(n)
search time.

Add a small slab cache (e.g., 8 slots × 64 bytes = 512 bytes) to
`zpico-alloc`. Allocations ≤ 64 bytes check the slab first (O(1)
bitmap scan). Frees return to slab if the pointer is in the slab
region. Larger allocations fall through to the free-list unchanged.

This makes the per-message string alloc/free pattern deterministic
without changing zenoh-pico code.

**Files**:
- `packages/zpico/zpico-alloc/src/lib.rs`

### 73.4 — `RecvGuard` trait and zenoh-pico backend implementation

Add `lock_recv_buffer()` to the `Subscriber` trait in `nros-rmw`.
Implement for `ZenohSubscriber` using the existing
`SUBSCRIBER_BUFFERS[i].locked` atomic flag.

When locked, the zenoh-pico direct-write callback skips the buffer
(drops the message). This is correct for `KEEP_LAST(1)` QoS and
matches the existing overflow behavior.

**Files**:
- `packages/core/nros-rmw/src/traits.rs` (add trait method + `RecvGuard`)
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs` (implement for zenoh)

### 73.5 — Borrowed message codegen (`MsgRef<'a>` types)

Extend the Rust codegen to generate `<Msg>Ref<'a>` types alongside
the existing owned types. For each unbounded sequence field, generate
`&'a [T]` instead of `heapless::Vec<T, N>`. For unbounded string
fields, generate `&'a str` instead of `heapless::String<N>`.

Fixed-size fields (u32, f64, bool, bounded arrays) remain owned
(copied during deserialization) — they're small and must be aligned.

Add a `deserialize_borrowed` function that takes `&'a [u8]` (CDR
buffer) and returns `MsgRef<'a>` with slices pointing into the buffer.

For non-`u8` element types (`float32[] ranges`), alignment must be
verified. CDR guarantees 4-byte alignment for `float32`, but the
buffer base may not be 4-byte aligned on all platforms. If alignment
cannot be guaranteed, fall back to copying those fields.

**Files**:
- `packages/codegen/packages/rosidl-codegen/src/types.rs`
- `packages/codegen/packages/rosidl-codegen/src/rust_gen.rs`

### 73.6 — Executor zero-copy dispatch path

Add an alternative subscription entry type in the executor arena that
uses `lock_recv_buffer()` + `deserialize_borrowed()` instead of
`try_recv_raw()` + arena buffer + owned deserialization.

The subscription registration API gains a zero-copy variant:

```rust
executor.add_subscription_zero_copy::<ImageRef, _>("/camera/image", |msg| {
    // msg borrows from RMW buffer, valid only in this callback
});
```

The existing `add_subscription` (copy-based, owned types) remains as
the default for backwards compatibility and for subscribers that need
to store messages.

`SubEntry` for zero-copy subscriptions omits the `buffer: [u8; RX_BUF]`
field, shrinking the arena footprint.

**Files**:
- `packages/core/nros-node/src/executor.rs`
- `packages/core/nros-node/src/executor/arena.rs`

### 73.7 — DDS and XRCE-DDS `RecvGuard` implementations

Implement `lock_recv_buffer()` for the DDS backend (`nros-rmw-dds`)
and XRCE-DDS backend (`nros-rmw-xrce`). Both have internal receive
buffers that can be exposed through the same guard pattern.

**Files**:
- `packages/dds/nros-rmw-dds/src/subscription.rs`
- `packages/xrce/nros-rmw-xrce/src/lib.rs`

### 73.8 — Reduce default `ZPICO_MAX_SUBSCRIBERS` and document sizing

The default `ZPICO_MAX_SUBSCRIBERS` (128) pre-allocates 128 subscriber
buffer slots. Most applications use 2–8 subscribers. Reduce the default
to 16 and document how to size it for the application.

Also document the relationship between buffer sizes and memory usage
in the embedded tuning guide.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs` (or zpico-sys build config)
- `docs/reference/environment-variables.md`
- `book/src/reference/embedded-tuning.md`

## Acceptance Criteria

- [ ] FreeRTOS `z_realloc` works (alloc + memcpy + free)
- [ ] ThreadX has a Rust `GlobalAlloc` implementation
- [ ] `zpico-alloc` slab fast-path passes allocation benchmarks
      (O(1) for ≤ 64 B, no regression for larger sizes)
- [ ] `sensor_msgs/Image` can be received on a 256 KB RAM target
      using borrowed deserialization without `alloc`
- [ ] Subscription receive path has zero memcpy for payload data
      when using the zero-copy API
- [ ] All existing tests pass (no regressions in copy-based path)
- [ ] `RecvGuard` implemented for zenoh-pico, DDS, and XRCE-DDS
- [ ] Default `ZPICO_MAX_SUBSCRIBERS` reduced; sizing documented

## Notes

- The owned message types and copy-based receive path are **not
  removed**. They remain the default. Zero-copy is opt-in per
  subscription via `add_subscription_zero_copy`.
- The borrowed `MsgRef<'a>` type cannot be stored beyond the callback.
  Users who need to keep a message must use the owned API or copy
  fields manually.
- The slab fast-path in `zpico-alloc` is transparent to zenoh-pico.
  No C code changes are needed.
- `z_realloc` fix (73.1) patches zenoh-pico's vendored C source.
  This must be re-applied when updating the zenoh-pico submodule.
- For non-`u8` borrowed sequences (e.g., `&'a [f32]`), alignment is
  platform-dependent. The codegen should emit a runtime alignment
  check and fall back to owned deserialization if the buffer base is
  misaligned. On Cortex-M with unaligned access support, this is a
  non-issue.
