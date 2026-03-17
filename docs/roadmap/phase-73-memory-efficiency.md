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

### Target receive path (zero-copy, triple-buffered)

```
Network → triple.write_buf → atomic swap → triple.read_buf → borrow-deserialize → callback
             (single write)   (no copy)      (no copy)          (slices into buf)
```

## Architecture

### QoS-driven buffer strategy

The subscriber's QoS depth (known at registration time) selects the
buffer strategy. The buffer slots live inside the executor arena — no
separate `SUBSCRIBER_BUFFERS` static array.

| QoS                   | Strategy               | Slots | Semantics                                          |
|-----------------------|------------------------|-------|----------------------------------------------------|
| `KEEP_LAST(1)`        | Triple buffer          | 3     | Latest value, writer never blocks, no message loss |
| `KEEP_LAST(N)`, N > 1 | SPSC ring of N+1 slots | N+1   | Ordered history, bounded, drops only when full     |
| `KEEP_ALL`            | Error on embedded      | —     | Unbounded not supported without `alloc`            |

**Triple buffer** (default for sensor data):

Three buffers rotate through write → middle → read roles via atomic
swaps. The writer (zenoh-pico callback) always has a buffer to write
into and never blocks. The reader (executor dispatch) always gets the
latest complete message. Intermediate messages may be skipped, matching
ROS 2's default `KEEP_LAST(1)` QoS semantics.

```
Writer: write to write_buf → atomic swap(write_buf, middle_buf)
Reader: atomic swap(middle_buf, read_buf) → deserialize from read_buf
```

No lock, no drop, no contention. Cost: 3 × buffer_size per subscriber.

**SPSC ring** (for command streams, action feedback):

A fixed-capacity ring with atomic head/tail pointers. The writer
advances head; the reader advances tail. Drops only when the ring is
full (head catches tail), which is bounded and predictable — it
depends on ring depth, not on callback duration.

### Dual message types: borrowed + owned

For messages with all fixed-size fields (e.g., `std_msgs/Int32`), a
single type is generated — no lifetime, identical to today.

For messages with unbounded strings or sequences, the codegen generates
**two types**:

```rust
/// Borrowed — zero-copy receive, lightweight publishing
pub struct Image<'a> {
    pub height: u32,
    pub width: u32,
    pub encoding: &'a str,
    pub data: &'a [u8],
}

/// Owned — storage, incremental construction, parameter services
pub struct ImageOwned {
    pub height: u32,
    pub width: u32,
    pub encoding: heapless::String<256>,
    pub data: heapless::Vec<u8, 64>,
}
```

**Conversions** follow Rust conventions:

```rust
// Borrowed → Owned (explicit copy)
let owned: ImageOwned = msg.to_owned();

// Owned → Borrowed (free, just borrows)
let borrowed: Image<'_> = owned.as_ref();
```

**Both types implement `Serialize` + `RosMessage`**, so either can be
published. `ImageOwned` also implements `Deserialize` (old copy-into-
struct path). `Image<'a>` has `deserialize_borrowed()` (zero-copy).

#### Rust API

```rust
// ── Subscription (borrowed, zero-copy) ──
executor.add_subscription::<Image>("/camera/image", qos, |msg: &Image<'_>| {
    process_pixels(msg.data);           // zero-copy from triple buffer
    let saved: ImageOwned = msg.to_owned(); // explicit copy if needed
});

// ── Publishing (borrowed — ergonomic, no capacity limits) ──
let pixels = capture_frame();
publisher.publish(&Image {
    height: 480, width: 640,
    encoding: "rgb8",
    data: &pixels,
})?;

// ── Publishing (owned — for pre-built messages) ──
publisher.publish(&owned_img.as_ref())?;

// ── Service (request borrowed, response owned) ──
executor.add_service::<GetParameters>("/node/get_parameters",
    |req: &GetParametersRequest<'_>| -> GetParametersResponseOwned {
        let mut resp = GetParametersResponseOwned::default();
        for name in req.names { /* name: &str, borrowed */ }
        resp
    },
);
```

#### C representation

```c
// Borrowed struct (pointer+length for unbounded fields)
typedef struct {
    uint32_t height;
    uint32_t width;
    struct { const char* data; size_t size; } encoding;
    struct { const uint8_t* data; size_t size; } data;
} sensor_msgs_msg_image_t;

// Subscription callback — pointers valid for callback duration
void on_image(const uint8_t* cdr_data, size_t len, void* ctx) {
    sensor_msgs_msg_image_t msg;
    sensor_msgs_msg_image_deserialize(&msg, cdr_data, len);
    process_pixels(msg.data.data, msg.data.size); // zero-copy
}

// Publishing — set pointers to local data
sensor_msgs_msg_image_t img = {0};
img.height = 480;
img.encoding.data = "rgb8";
img.encoding.size = 4;
img.data.data = pixels;
img.data.size = pixel_count;
// serialize and publish...
```

#### C++ representation (freestanding C++14)

Uses `nros::Span<T>` and `nros::StringView` from `nros/span.hpp`:

```cpp
namespace sensor_msgs::msg {
struct Image {
    uint32_t height;
    uint32_t width;
    nros::StringView encoding;
    nros::Span<uint8_t> data;
};
} // namespace sensor_msgs::msg

void on_image(const sensor_msgs::msg::Image& msg) {
    for (uint8_t pixel : msg.data) { ... }  // range-for works
    if (msg.encoding.equals("rgb8")) { ... }
}
```

### Arena-based buffer allocation

Subscription buffer slots are allocated inside the executor arena at
registration time. The arena is a compile-time-sized bump allocator
that already holds heterogeneous entry types of different sizes. Adding
a variable-length trailing buffer region is a natural extension.

**Arena entry layout for a triple-buffered subscription:**

```
Arena (NROS_EXECUTOR_ARENA_SIZE bytes, compile-time sized)
┌──────────────────────────────────────────────────────────────────┐
│ SubTripleEntry struct   │ slot 0 [BUF_SIZE] │ slot 1 │ slot 2   │
│  - handle               │                   │        │          │
│  - buf_ptr ─────────────┘                   │        │          │
│  - buf_size                                 │        │          │
│  - state: AtomicU8 (write/middle/read index)│        │          │
│  - lengths: [AtomicUsize; 3]                │        │          │
│  - callback: F                              │        │          │
├──────────────────────────────────────────────────────────────────┤
│ SrvEntry for /get_params │ [req_buf] │ [reply_buf]              │
├──────────────────────────────────────────────────────────────────┤
│ SubRingEntry struct     │ slot 0 │ slot 1 │ ... │ slot N        │
│  - handle               │        │        │     │               │
│  - buf_ptr ─────────────┘        │        │     │               │
│  - buf_size, depth               │        │     │               │
│  - head: AtomicUsize             │        │     │               │
│  - tail: AtomicUsize             │        │     │               │
│  - callback: F                   │        │     │               │
├──────────────────────────────────────────────────────────────────┤
│ (remaining free space)                                           │
└──────────────────────────────────────────────────────────────────┘
```

At registration, the arena bump allocator places:
1. The entry struct (fixed size, contains metadata + callback)
2. Immediately after: `num_slots × buf_size` bytes for the buffer slots
3. `buf_ptr` in the struct points to the start of step 2

The QoS depth determines `num_slots` at runtime (3 for triple buffer,
`depth + 1` for SPSC ring). `buf_size` is a const generic
(`NROS_SUBSCRIPTION_BUFFER_SIZE`). The total arena consumption is
predictable: `size_of::<Entry>() + num_slots × buf_size`.

If the arena is too small for the requested depth, registration returns
`NodeError::ArenaFull` — fail-fast at init, not silent degradation at
runtime. Users control total memory via `NROS_EXECUTOR_ARENA_SIZE`.

This replaces the separate `SUBSCRIBER_BUFFERS` static array. Buffer
memory is allocated only for subscribers that are actually registered,
not for 128 pre-allocated slots.

### Shim integration (not an RMW trait change)

The zenoh-pico shim's direct-write callback writes into the triple
buffer's write slot (obtained via `buf_ptr + write_index * buf_size`).
After writing, it performs the atomic swap. The shim does not need a
lock flag — the triple buffer protocol is inherently lock-free.

The RMW `Subscriber` trait is **not changed**. The triple buffer /
SPSC ring is an executor-level concern. Each backend's shim writes
into whichever slot the buffer strategy designates, coordinated by
the atomic state in the entry struct.

For backends that don't support direct-write (e.g., DDS with internal
queuing), the executor falls back to calling `try_recv_raw()` to copy
into the read slot. This is still one fewer copy than today (no
separate `SUBSCRIBER_BUFFERS` → arena copy).

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

### Allocator fixes (done)

- [x] 73.1 — Fix FreeRTOS `z_realloc` (returns NULL)
- [x] 73.2 — Fix ThreadX missing Rust `GlobalAlloc`
- [x] 73.3 — Slab fast-path in `zpico-alloc`

### Buffer primitives and executor integration (done)

- [x] 73.4 — Triple buffer primitive
- [x] 73.5 — SPSC ring buffer primitive
- [x] 73.6 — Arena-based buffer allocation for subscriptions
- [x] 73.7 — Zenoh shim drain into triple buffer
- [x] 73.8 — Borrowed message codegen (Rust)
- [x] 73.8a — Dual-type codegen: generate `Msg<'a>` + `MsgOwned` + conversions
- [x] 73.9 — Borrowed message codegen (C/C++) and `nros::Span` header
- [x] 73.10 — Executor zero-copy dispatch path
- [x] 73.11 — DDS and XRCE-DDS shim integration

### Alloc removal (done)

- [x] 73.12 — Remove `alloc` from zero-copy subscriber
- [x] 73.13 — Remove `alloc` from timer callbacks
- [x] 73.14 — Remove `alloc` from large service replies
- [x] 73.15 — Remove `alloc` from zenoh ID formatting and executor config

### API migration (done)

- [x] 73.16 — Deprecate owned-type subscription API
- [x] 73.17 — C/C++ borrowed subscription API (codegen templates)

### Serialization prerequisites (done)

- [x] 73.18 — Add `CdrReader::read_slice_*` methods to nros-serdes
- [x] 73.19 — Add `nros_cdr_write_string_n` to C CDR library

### Codegen improvements (not started)

- [x] 73.19a — Codegen: propagate lifetime to nested type references
- [x] 73.19b — Codegen: add `--rename` option for package name remapping

### Codegen fixes for rcl-interfaces

- [x] 73.19c — Codegen: re-export `*Owned` types from `msg/mod.rs`
- [x] 73.19d — Multi-byte primitives: use heapless::Vec (not `&'a [T]`)
- [x] 73.19e — Codegen: fix `to_owned()` for `&'a [Nested<'a>]` sequences
- [x] 73.19f — Remove `Serialize + Deserialize` from `RosMessage` trait bound

### rcl-interfaces migration (blocked on 73.19f)

- [ ] 73.20a — Regenerate rcl-interfaces with dual-type codegen
- [ ] 73.20b — Migrate `parameter_services.rs` to use `*Owned` types
- [ ] 73.20c — Update workspace Cargo.toml paths for regenerated rcl-interfaces

### Example migration (blocked on 73.20a–73.20c)

- [ ] 73.20 — Regenerate bindings and migrate Rust examples
- [ ] 73.21 — Regenerate bindings and migrate C/C++ examples

### Final cleanup (blocked on 73.20–73.21)

- [ ] 73.22 — Remove `SUBSCRIBER_BUFFERS` and old subscription API
- [ ] 73.23 — Document sizing and migration

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

### 73.4 — Triple buffer primitive

Implement a `no_std`, lock-free triple buffer in a new module within
`nros-node` (or a shared crate). The triple buffer manages three
equally-sized byte slots using a single `AtomicU8` that encodes the
write/middle/read buffer indices.

API:

```rust
pub struct TripleBuffer {
    buf_ptr: *mut u8,       // base of the 3 × slot_size region
    slot_size: usize,
    state: AtomicU8,        // packed write_idx:2 | middle_idx:2 | read_idx:2 | dirty:1
    lengths: [AtomicUsize; 3],
}

impl TripleBuffer {
    /// Initialize over a pre-allocated memory region.
    pub unsafe fn init(buf_ptr: *mut u8, slot_size: usize) -> Self;

    /// Get a mutable slice for the writer to fill. Never blocks.
    pub fn write_slot(&self) -> &mut [u8];

    /// Writer is done — swap write and middle slots.
    pub fn writer_publish(&self, len: usize);

    /// Swap middle and read if new data available. Returns the
    /// read slot and its length, or None if no new data.
    pub fn reader_acquire(&self) -> Option<(&[u8], usize)>;
}
```

The triple buffer does not own its memory — it operates on a region
provided by the arena. This keeps it independent of the allocation
strategy.

**Files**:
- `packages/core/nros-node/src/executor/triple_buffer.rs`

### 73.5 — SPSC ring buffer primitive

Implement a `no_std`, lock-free SPSC ring buffer for `KEEP_LAST(N)`
with N > 1. Uses atomic head/tail indices over a pre-allocated region
of `(N+1) × slot_size` bytes (one extra slot for the full/empty
disambiguation).

API:

```rust
pub struct SpscRing {
    buf_ptr: *mut u8,
    slot_size: usize,
    capacity: usize,        // N+1
    head: AtomicUsize,      // writer position
    tail: AtomicUsize,      // reader position
    lengths: *mut usize,    // per-slot data length, in trailing region
}

impl SpscRing {
    pub unsafe fn init(buf_ptr: *mut u8, slot_size: usize, depth: usize) -> Self;
    pub fn try_push(&self) -> Option<&mut [u8]>;
    pub fn commit_push(&self, len: usize);
    pub fn try_pop(&self) -> Option<(&[u8], usize)>;
    pub fn commit_pop(&self);
}
```

**Files**:
- `packages/core/nros-node/src/executor/spsc_ring.rs`

### 73.6 — Arena-based buffer allocation for subscriptions

Modify the executor's `add_subscription` to allocate buffer slots from
the arena at registration time, based on QoS depth:

```rust
// Registration places entry + trailing buffer in arena
let slot_count = match qos.history {
    KeepLast if qos.depth <= 1 => 3,          // triple buffer
    KeepLast => (qos.depth as usize) + 1,     // SPSC ring
    KeepAll => return Err(NodeError::UnsupportedQos),
};

let entry_size = size_of::<SubEntry<M, F>>();
let buffer_region = slot_count * RX_BUF;
let total = entry_size + buffer_region;

let offset = arena.bump_alloc(total, align_of::<SubEntry<M, F>>())?;
let buf_ptr = arena.as_mut_ptr().add(offset + entry_size);

// Write entry struct with buf_ptr pointing to trailing region
ptr::write(arena[offset] as *mut SubEntry, SubEntry {
    handle,
    buf_ptr,
    buf_size: RX_BUF,
    buffer: match slot_count {
        3 => BufferStrategy::Triple(TripleBuffer::init(buf_ptr, RX_BUF)),
        n => BufferStrategy::Ring(SpscRing::init(buf_ptr, RX_BUF, n - 1)),
    },
    callback,
    ..
});
```

The `SubEntry` struct no longer contains a `buffer: [u8; RX_BUF]`
inline array. Instead, it holds a `buf_ptr` to the trailing region
and a `BufferStrategy` enum (triple buffer or SPSC ring).

**Files**:
- `packages/core/nros-node/src/executor/arena.rs`
- `packages/core/nros-node/src/executor.rs`

### 73.7 — Zenoh shim direct-write into triple buffer

Modify the zenoh shim's `declare_subscriber_direct_write_raw()` call
to write into the triple buffer's write slot instead of the static
`SUBSCRIBER_BUFFERS` array.

The shim callback becomes:

```rust
unsafe extern "C" fn subscriber_notify_callback(
    data: *const u8, len: usize, ctx: *mut c_void,
) {
    let triple = &*(ctx as *const TripleBuffer);
    let slot = triple.write_slot();
    let copy_len = len.min(slot.len());
    ptr::copy_nonoverlapping(data, slot.as_mut_ptr(), copy_len);
    triple.writer_publish(copy_len);
}
```

The ctx pointer passed to zenoh-pico points to the `TripleBuffer` (or
`SpscRing`) inside the arena entry, rather than to a
`SUBSCRIBER_BUFFERS` index.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`

### 73.8 — Borrowed message codegen (Rust)

Change the Rust codegen to generate **one type per message** (not two).
If all fields are fixed-size, the type has no lifetime (unchanged from
today). If the message contains unbounded strings or sequences, the
type carries a lifetime parameter:

```rust
// All fixed-size — no lifetime, no change
pub struct Int32 { pub data: i32 }

// Has unbounded fields — single type with lifetime
pub struct Image<'a> {
    pub height: u32,
    pub width: u32,
    pub encoding: &'a str,
    pub data: &'a [u8],
}
```

The same type serves both receiving (borrows from transport buffer)
and publishing (borrows from user's local data). Implement both
`Serialize` (reads `&str` / `&[u8]` and writes to CDR) and
`DeserializeBorrowed` (reads CDR and returns slices into the source
buffer).

For non-`u8` element types (`float32[] ranges`), alignment must be
verified. CDR guarantees 4-byte alignment for `float32`, but the
buffer base may not be 4-byte aligned on all platforms. If alignment
cannot be guaranteed, fall back to copying those fields into a
`heapless::Vec` (bounded sequences) or returning an error (unbounded).

**Files**:
- `packages/codegen/packages/rosidl-codegen/src/types.rs`
- `packages/codegen/packages/rosidl-codegen/src/rust_gen.rs`
- `packages/core/nros-serdes/src/lib.rs` (add `DeserializeBorrowed` trait)

### 73.9 — Borrowed message codegen (C/C++) and `nros::Span` header

#### C codegen

Generate C message structs with `const pointer + size_t` pairs for
unbounded string/sequence fields:

```c
typedef struct {
    uint32_t height;
    uint32_t width;
    const char* encoding;       // points into CDR buffer
    size_t encoding_len;
    const uint8_t* data;        // points into CDR buffer
    size_t data_len;
} sensor_msgs_msg_image_t;
```

Fixed-size fields remain value types. The deserializer populates
pointer fields to point into the CDR buffer; the struct is valid
only for the duration of the subscription callback.

#### C++ codegen

Add a freestanding `nros/span.hpp` header with `nros::Span<T>` and
`nros::StringView` — lightweight non-owning view types compatible
with C++14 (GCC 5+, Clang 3.5+):

```cpp
namespace nros {

template <typename T>
struct Span {
    const T* ptr;
    size_t len;
    constexpr const T* data() const { return ptr; }
    constexpr size_t size() const { return len; }
    constexpr bool empty() const { return len == 0; }
    constexpr const T& operator[](size_t i) const { return ptr[i]; }
    constexpr const T* begin() const { return ptr; }
    constexpr const T* end() const { return ptr + len; }
};

struct StringView {
    const char* ptr;
    size_t len;
    constexpr const char* data() const { return ptr; }
    constexpr size_t size() const { return len; }
    constexpr bool empty() const { return len == 0; }
    constexpr const char* begin() const { return ptr; }
    constexpr const char* end() const { return ptr + len; }
};

} // namespace nros
```

Generated C++ message structs use these types for unbounded fields:

```cpp
namespace sensor_msgs::msg {
struct Image {
    uint32_t height;
    uint32_t width;
    nros::StringView encoding;
    nros::Span<uint8_t> data;
};
} // namespace sensor_msgs::msg
```

Range-based `for`, `data()`, `size()`, and indexing work out of the
box. C++20 users can convert to `std::span` trivially. In
`NROS_CPP_STD` mode, convenience methods for `std::string` /
`std::vector` conversion are available.

No C++17 or C++20 requirement — works on all embedded toolchains.

**Files**:
- `packages/core/nros-cpp/include/nros/span.hpp` (new)
- `packages/codegen/packages/rosidl-codegen/src/c_gen.rs`
- `packages/codegen/packages/rosidl-codegen/src/cpp_gen.rs`

### 73.10 — Executor zero-copy dispatch path

Modify the executor dispatch to read from the triple buffer's read
slot and deserialize borrowed:

```rust
// Triple buffer path (KEEP_LAST(1))
if let Some((data, len)) = entry.buffer.reader_acquire() {
    let msg: Image<'_> = deserialize_borrowed(&data[..len]);
    (entry.callback)(&msg);
    // read slot remains valid until next reader_acquire()
}

// SPSC ring path (KEEP_LAST(N))
while let Some((data, len)) = entry.buffer.try_pop() {
    let msg: Image<'_> = deserialize_borrowed(&data[..len]);
    (entry.callback)(&msg);
    entry.buffer.commit_pop();
}
```

The subscription registration API:

```rust
// Uses QoS depth to pick triple buffer or SPSC ring automatically
executor.add_subscription::<Image>("/camera/image", QosSettings::SENSOR_DATA, |msg| {
    // msg borrows from arena buffer slot, valid for callback duration
    process_pixels(msg.data);
});
```

The existing copy-based `add_subscription` with owned types remains
available for backwards compatibility (messages without lifetime
parameters work exactly as today).

**Files**:
- `packages/core/nros-node/src/executor.rs`
- `packages/core/nros-node/src/executor/arena.rs`

### 73.11 — DDS and XRCE-DDS shim integration

For backends that don't support direct-write into the arena buffer,
the executor calls `try_recv_raw()` to copy into the write slot, then
publishes. This is still one copy (into the arena) instead of two
(SUBSCRIBER_BUFFERS → arena).

For DDS (dust-dds), investigate whether the subscriber reader can
write directly into the triple buffer's write slot via a custom
`DataReaderListener`. For XRCE-DDS, the stream callback can be
redirected similarly.

**Files**:
- `packages/dds/nros-rmw-dds/src/subscription.rs`
- `packages/xrce/nros-rmw-xrce/src/lib.rs`

### 73.12 — Remove `alloc` from zero-copy subscriber

Deprecated `ZenohZeroCopySubscriber` (requires `alloc` for `Box<dyn>`
callback). The new `add_subscription_buffered_raw` provides zero-copy
without `alloc`. Full removal deferred to 73.22.

**Files**: `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`

### 73.13 — Remove `alloc` from timer callbacks

Removed `TimerCallback` (`Box<dyn FnMut()>`), `callback_box` field,
`new_with_box()`, `set_callback_box()`. Executor uses `TimerEntry<F>`
with concrete closures — the `Box<dyn>` path was unused.

**Files**: `packages/core/nros-node/src/timer.rs`

### 73.14 — Remove `alloc` from large service replies

Gated `handle_request_boxed()` on `param-services` instead of `alloc`.
Only parameter services use it.

**Files**: `packages/core/nros-node/src/executor/handles.rs`

### 73.15 — Remove `alloc` from zenoh ID formatting and executor config

Removed dead `to_hex_string()` (callers use `to_hex_bytes()`). Changed
`ExecutorConfig::from_env()` gate from `std + alloc` to `std` only.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/zpico.rs`
- `packages/core/nros-node/src/executor/types.rs`

### 73.16 — Deprecate owned-type subscription API

`add_subscription` and `add_subscription_raw` now delegate to the
buffered entry types internally (triple buffer, `KEEP_LAST(1)`).
`SubEntry` and `SubRawEntry` marked deprecated. Arena sizing updated
to account for 3× buffer per subscription.

**Files**:
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/executor/arena.rs`
- `packages/core/nros-node/build.rs`

### 73.17 — C/C++ borrowed subscription API (codegen templates)

Updated C deserialization template to set pointers into CDR buffer for
unbounded strings/sequences (zero-copy). Updated C serialization to
work with pointer+length fields. Added `is_unbounded_string` and
`is_unbounded_sequence` flags to `CField`.

**Files**:
- `packages/codegen/packages/rosidl-codegen/src/templates.rs`
- `packages/codegen/packages/rosidl-codegen/src/generator/common.rs`
- `packages/codegen/packages/rosidl-codegen/templates/message_c.c.jinja`

### 73.18 — Add `CdrReader::read_slice_*` methods to nros-serdes

The generated `deserialize_borrowed()` for primitive sequences calls
`reader.read_slice_u8()`, `reader.read_slice_f32()`, etc. These
methods need to return `&'a [T]` slices pointing into the CDR buffer.

For each primitive type, add a method that:
1. Reads the sequence length (u32)
2. Validates the buffer has enough bytes
3. Returns a `&'a [T]` slice into the buffer (zero-copy for u8;
   alignment check for larger primitives)

**Files**: `packages/core/nros-serdes/src/cdr.rs`

### 73.19 — Add `nros_cdr_write_string_n` to C CDR library

The generated C serializer for unbounded strings calls
`nros_cdr_write_string_n(ptr, end, origin, data, len)` to serialize
from a `const char* + size_t` pair instead of a null-terminated string.

Implement this function alongside the existing `nros_cdr_write_string`.

**Files**: `packages/core/nros-c/include/nano_ros/cdr.h`

### 73.19a — Codegen: propagate lifetime to nested type references

**Blocking issue discovered during 73.20a attempt.**

When `ParameterValue` has unbounded fields and becomes
`ParameterValue<'a>`, any message that references it (e.g.,
`Parameter { value: ParameterValue }`) must also propagate the
lifetime: `Parameter<'a> { value: ParameterValue<'a> }`.

The current codegen generates each message independently and doesn't
know whether a referenced nested type has a lifetime parameter. This
causes compilation errors like:
```
error[E0106]: missing lifetime specifier
  --> parameter.rs:12:28
   |
12 |     pub value: crate::msg::ParameterValue,
   |                            ^^^^^^^^^^^^^^ expected named lifetime parameter
```

**Fix**: Add a two-pass approach to the codegen:
1. First pass: identify which message types need lifetimes (have
   unbounded fields directly or transitively via nested types)
2. Second pass: generate code with correct lifetime propagation

Alternatively, always generate nested type references with a lifetime
parameter when the nested type is known to have unbounded fields
(requires building a dependency graph of message types).

**Files**:
- `packages/codegen/packages/rosidl-codegen/src/generator/msg.rs`
- `packages/codegen/packages/rosidl-codegen/src/types.rs`

### 73.19b — Codegen: add `--crate-prefix` option for package name remapping

The codegen generates crate names matching ROS package names
(`rcl_interfaces`, `builtin_interfaces`). The nano-ros project uses
`nros-` prefixed names (`nros-rcl-interfaces`, `nros-builtin-interfaces`)
to avoid conflicts with user-generated bindings.

Add a `--crate-prefix` option to `cargo nano-ros generate` that
prepends a prefix to generated crate names, dependency references,
and `use` statements. Example:

```bash
cargo nano-ros generate --crate-prefix nros -o generated/humble
```

Generates `nros-rcl-interfaces` (crate name), with dependency
`nros-builtin-interfaces` (not `builtin_interfaces`), and
`use nros_builtin_interfaces::msg::Time` (not `builtin_interfaces`).

Also add `--nros-path` to override the dependency paths for
`nros-core` and `nros-serdes` (currently hardcoded to `version = "*"`
or relative path).

**Files**:
- `packages/codegen/packages/cargo-nano-ros/src/main.rs`
- `packages/codegen/packages/rosidl-codegen/src/generator/msg.rs`

### 73.19c — Codegen: re-export `*Owned` types from `msg/mod.rs`

The generated `msg/mod.rs` only re-exports the borrowed type (e.g.,
`pub use parameter::Parameter`). It must also re-export the owned
variant (`pub use parameter::ParameterOwned`) for cross-module
references like `crate::msg::ParameterOwned` to resolve.

**Files**: `rosidl-codegen` mod.rs template / generator

### 73.19d — Codegen: add `read_slice_i64`, `read_slice_f64` to CdrReader

The `ParameterValue` type contains `int64[] integer_array_value` and
`float64[] double_array_value`. The generated `deserialize_borrowed`
calls `reader.read_slice_i64()` and `reader.read_slice_f64()` which
don't exist. Currently only `read_slice_u8` and `_raw` variants exist.

Add `read_slice_i64`, `read_slice_f64` (and other primitives) that
return `&'a [T]` with proper alignment handling. Or change the template
to use the `_raw` variants and document the endianness constraint.

**Files**: `packages/core/nros-serdes/src/cdr.rs`

### 73.19e — Codegen: fix `to_owned()` for `&'a [Nested<'a>]` sequences

The template generates `self.field.to_owned()` for lifetime-nested
fields. When the field is `&'a [Parameter<'a>]`, Rust's built-in
`ToOwned::to_owned` returns `Vec<Parameter<'a>>` — wrong. It should
call the generated `to_owned()` method on each element and collect
into a `heapless::Vec<ParameterOwned, N>`.

Fix the template to generate proper per-element conversion for
sequences of lifetime-nested types.

**Files**: `message_nros.rs.jinja` template

### 73.19f — Lifetime-parameterized `Deserialize<'de>` trait

**Blocking issue discovered during 73.20a attempt.**

The current `Deserialize` trait is:
```rust
pub trait Deserialize: Sized {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError>;
}
```

`RosMessage` requires `Deserialize`, but borrowed types like
`ParameterValue<'a>` can't implement `Deserialize` — they need to
borrow from the reader's buffer, which requires a lifetime:

```rust
// Needed (serde-style lifetime-parameterized deserialization):
pub trait Deserialize<'de>: Sized {
    fn deserialize(reader: &mut CdrReader<'de>) -> Result<Self, DeserError>;
}
```

With this, `ParameterValue<'a>` implements `Deserialize<'a>` (borrows
from buffer), and `ParameterValueOwned` implements
`for<'de> Deserialize<'de>` (copies from buffer, no lifetime).

`RosMessage` becomes:
```rust
pub trait RosMessage: Sized + Serialize {
    const TYPE_NAME: &'static str;
    const TYPE_HASH: &'static str;
}
```

(Removing the `Deserialize` bound — deserialization is handled
separately through `Deserialize<'de>` or `deserialize_borrowed`.)

**Scope**: Changes `nros-core`, `nros-serdes`, all generated code,
executor dispatch, C/C++ FFI wrappers. Large refactor.

**Files**:
- `packages/core/nros-core/src/types.rs` (trait definitions)
- `packages/core/nros-serdes/src/traits.rs` (`Deserialize` trait)
- `packages/core/nros-serdes/src/primitives.rs` (primitive impls)
- `packages/core/nros-node/src/executor/arena.rs` (dispatch)
- All generated message/service code

### 73.20a — Regenerate rcl-interfaces with dual-type codegen

Depends on 73.19f.

Regenerate the checked-in rcl-interfaces bindings using `cargo nano-ros
clean && cargo nano-ros generate` in the rcl-interfaces package directory.

The output produces `Parameter<'a>` + `ParameterOwned`,
`SetParametersResult<'a>` + `SetParametersResultOwned`, etc.

Must update the generated output directory structure to match the
workspace member paths in the root `Cargo.toml`
(`generated/humble/nros-rcl-interfaces/`).

**Files**:
- `packages/interfaces/rcl-interfaces/generated/humble/nros-rcl-interfaces/`
- `packages/interfaces/rcl-interfaces/generated/humble/nros-builtin-interfaces/`

### 73.20b — Migrate `parameter_services.rs` to use `*Owned` types

The parameter services module constructs response structs (`Parameter`,
`SetParametersResult`, `ListParametersResult`, etc.). With the new
codegen, these become `Parameter<'a>` which requires a lifetime.

Migrate all response construction to use `*Owned` variants:
- `Parameter` → `ParameterOwned`
- `SetParametersResult` → `SetParametersResultOwned`
- `ParameterDescriptor` → `ParameterDescriptorOwned`
- `ListParametersResult` → `ListParametersResultOwned`
- `ParameterValue` → stays as-is (no unbounded fields, no lifetime)

Service handlers return owned responses; the executor serializes
them via `Serialize` (which delegates to `as_ref().serialize()`).

**Files**:
- `packages/core/nros-node/src/parameter_services.rs`
- `packages/core/nros-node/src/params.rs`

### 73.20c — Update workspace Cargo.toml paths for regenerated rcl-interfaces

If the generated directory structure changes (e.g., crate names or
paths differ from the checked-in version), update the root
`Cargo.toml` workspace members to match.

**Files**: `Cargo.toml`

### 73.20 — Regenerate bindings and migrate Rust examples

Depends on 73.20a–73.20c.

1. Regenerate all Rust example bindings with `just generate-bindings`
2. Update subscription callbacks for messages with borrowed fields
3. Examples that store messages call `.to_owned()`

**Files**: `examples/*/rust/zenoh/*/src/main.rs`

### 73.21 — Regenerate bindings and migrate C/C++ examples

Depends on 73.19.

**Files**:
- `examples/native/c/zenoh/*/`
- `examples/native/cpp/zenoh/*/`
- `examples/zephyr/*/`

### 73.22 — Remove `SUBSCRIBER_BUFFERS` and old subscription API

After all examples are migrated (73.20–73.21):

1. Remove `SubEntry`, `SubInfoEntry`, `SubSafetyEntry`, `SubRawEntry`
   and their dispatch/readiness/presample functions from arena.rs
2. Remove deprecated `add_subscription_with_info_sized` variants
3. Remove `SUBSCRIBER_BUFFERS`, `SubscriberBuffer`, `SubscriberBufferRef`,
   `NEXT_BUFFER_INDEX`, `subscriber_notify_callback` from zenoh shim
4. Modify zenoh shim to write directly into the triple buffer's write
   slot (callback ctx points to `TripleBuffer` in the arena)
5. Remove `ZPICO_MAX_SUBSCRIBERS` as a buffer pre-allocation constant

This eliminates the 133 KB static memory waste and completes the
migration to arena-based buffering.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/executor/arena.rs`

### 73.23 — Document sizing and migration

Update the embedded tuning guide with the new memory model:

- Arena sizing formula: `sum over subscriptions of
  (entry_overhead + slot_count × buf_size)`
- Default configuration memory footprint comparison (before/after)
- Migration guide for Rust, C, and C++ users
- QoS depth recommendations per use case
- How to copy borrowed fields into owned storage when needed

**Files**:
- `docs/reference/environment-variables.md`
- `book/src/reference/embedded-tuning.md`

## Acceptance Criteria

- [ ] FreeRTOS `z_realloc` works (alloc + memcpy + free)
- [ ] ThreadX has a Rust `GlobalAlloc` implementation
- [ ] `zpico-alloc` slab fast-path passes allocation benchmarks
      (O(1) for ≤ 64 B, no regression for larger sizes)
- [ ] `sensor_msgs/Image` can be received on a 256 KB RAM target
      using borrowed deserialization without `alloc`
- [ ] Triple buffer: writer never blocks, reader always gets latest,
      no message loss (verified by unit test with concurrent producer)
- [ ] SPSC ring: bounded drop only when full, ordered delivery
      (verified by unit test)
- [ ] Subscription receive path has zero memcpy for payload data
      when using borrowed message types
- [ ] Dual message types generated: `Image<'a>` + `ImageOwned` for Rust;
      pointer+length structs for C/C++
- [ ] `to_owned()` and `as_ref()` conversions generated and tested
- [ ] `nros::Span` and `nros::StringView` work on C++14 freestanding
      (GCC 5+, Clang 3.5+, no STL required)
- [x] Owned-type subscription API deprecated; all subscriptions use
      buffered entries (triple buffer / SPSC ring) internally
- [x] `CdrReader::read_slice_*` methods implemented and tested
- [x] `nros_cdr_write_string_n` implemented in C CDR library
- [ ] All Rust examples migrated to borrowed message types
- [ ] All C/C++ examples migrated to borrowed subscription API
- [ ] `SUBSCRIBER_BUFFERS` static array removed; memory usage reduced
- [ ] All existing tests pass (no regressions)
- [ ] `alloc` feature only required by `param-services`; all other
      nros functionality works without `alloc`
- [ ] `unstable-zenoh-api` zero-copy works without `alloc`
- [ ] `grep -r 'feature.*alloc' packages/core/ packages/zpico/nros-rmw-zenoh/`
      shows only `param-services`-related gates and `extern crate alloc`
      declarations
- [ ] Arena sizing documented with before/after comparison

## Notes

- **Dual types for unbounded messages**: Messages with unbounded
  fields generate `Image<'a>` (borrowed) + `ImageOwned` (owned).
  Messages with all fixed-size fields generate a single type (no
  lifetime, no Owned variant — identical to today).
- **Conversions**: `msg.to_owned() -> ImageOwned` (explicit copy),
  `owned.as_ref() -> Image<'_>` (free borrow). Both types implement
  `Serialize` + `RosMessage`, so either can be published.
- **Subscription callbacks** receive `&Image<'_>` (borrowed from
  triple buffer). Call `.to_owned()` to keep data beyond the callback.
- **Service handlers** receive borrowed requests, return owned
  responses. Parameter services use `*Owned` types internally.
- **C/C++ message types use pointer+length pairs** for borrowed
  fields. C++ uses `nros::Span<T>` and `nros::StringView` (C++14,
  no STL). No C++17 or C++20 required.
- The slab fast-path in `zpico-alloc` is transparent to zenoh-pico.
  No C code changes are needed.
- `z_realloc` fix (73.1) patches zenoh-pico's vendored C source.
  This must be re-applied when updating the zenoh-pico submodule.
- For non-`u8` borrowed sequences (e.g., `&'a [f32]`), alignment is
  platform-dependent. The codegen should emit a runtime alignment
  check and fall back to copying if the buffer base is misaligned.
  On Cortex-M with unaligned access support, this is a non-issue.
- On single-threaded bare-metal, the triple buffer atomics degenerate
  to plain reads/writes (compiler fence only). The same code works
  correctly on multi-threaded platforms (RTIC interrupt-driven,
  POSIX threaded) where the writer runs in an interrupt or I/O thread.
- `KEEP_ALL` QoS is rejected at registration time on embedded. Users
  who need unbounded queuing must use `alloc` + a heap-backed
  container outside the executor.
- **`alloc` after Phase 73**: The only feature requiring `alloc` is
  `param-services` (ROS 2 parameter service responses contain large
  heapless arrays that overflow the stack — `Box::new(response)` is
  unavoidable). All other nros functionality — pub/sub, services,
  actions, timers, zero-copy receive — works without `alloc`.
  The `alloc` feature flag, `extern crate alloc` declarations, and
  RTOS global allocators (FreeRTOS, ThreadX) remain for
  `param-services` and for users who opt into heap-based convenience
  APIs on hosted platforms.
- **Serdes `alloc` impls** (`Serialize`/`Deserialize` for
  `alloc::String` and `alloc::Vec<T>`) are retained — they are
  harmless (no code generated when `alloc` is disabled) and useful
  for hosted-platform users who mix `std` collections with nros
  messaging.
