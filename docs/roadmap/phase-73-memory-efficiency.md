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

### Borrowed message types (single type, not two variants)

The codegen generates **one type per message**. If all fields are
fixed-size (e.g., `std_msgs/Int32`), the type has no lifetime and is
identical to today. If the message contains unbounded strings or
sequences, the type carries a lifetime parameter with `&'a str` /
`&'a [T]` for those fields:

```rust
// Fixed-size only — no lifetime, identical to current codegen
pub struct Int32 {
    pub data: i32,
}

// Has unbounded fields — lifetime parameter, borrowed slices
pub struct Image<'a> {
    pub height: u32,
    pub width: u32,
    pub encoding: &'a str,    // borrows from CDR buffer (or user data)
    pub data: &'a [u8],       // borrows from CDR buffer (or user data)
}
```

`Image<'a>` is always small (fixed-size fields + pointer/length pairs).
The payload data stays in the buffer it borrows from. The lifetime `'a`
ties the message to that buffer's scope.

The same type works for both **receiving** and **publishing**:

```rust
// Receiving: borrows from transport buffer (arena triple buffer slot)
executor.add_subscription::<Image>("/camera/image", qos, |msg: &Image<'_>| {
    // msg.data borrows from arena read slot, valid for callback duration
    process_pixels(msg.data);
});

// Publishing: borrows from user's local data
let encoding = "rgb8";
let pixels: [u8; 1024] = capture_frame();
publisher.publish(&Image {
    height: 480,
    width: 640,
    encoding,       // &str borrows from local
    data: &pixels,  // &[u8] borrows from local
});
```

No separate `ImageRef` / `ImageOwned` split — one type serves both
directions. The `Serialize` impl on `Image<'a>` reads the borrowed
slices and writes them into CDR. The `DeserializeBorrowed` impl reads
CDR fields and returns slices into the source buffer.

#### C representation

C message structs use `const pointer + size_t` pairs for borrowed
fields — the standard C pattern for non-owning views:

```c
// Generated C struct
typedef struct {
    uint32_t height;
    uint32_t width;
    const char* encoding;           // points into CDR buffer
    size_t encoding_len;
    const uint8_t* data;            // points into CDR buffer
    size_t data_len;
} sensor_msgs_msg_image_t;

// Subscription callback — pointer valid for callback duration
void on_image(const sensor_msgs_msg_image_t* msg, void* ctx) {
    process_frame(msg->data, msg->data_len);
}
```

The C user already understands this pattern (same as `recv()` giving
a pointer into a buffer). The "lifetime" is the callback scope.

#### C++ representation (freestanding C++14)

C++ message structs use `nros::Span<T>` and `nros::StringView` —
lightweight non-owning view types that provide the same UX as
`std::span` / `std::string_view` without requiring C++17/C++20.

These are defined in a single header (`nros/span.hpp`), ~20 lines
total, compatible with GCC 5+ and Clang 3.5+:

```cpp
// nros/span.hpp — freestanding, no STL required
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

Generated C++ message type:

```cpp
namespace sensor_msgs::msg {

struct Image {
    uint32_t height;
    uint32_t width;
    nros::StringView encoding;    // borrows from CDR buffer
    nros::Span<uint8_t> data;     // borrows from CDR buffer
};

} // namespace sensor_msgs::msg
```

User code:

```cpp
void on_image(const sensor_msgs::msg::Image& msg) {
    // Range-based for loop (begin/end defined)
    for (uint8_t pixel : msg.data) { ... }

    // Direct pointer access
    process_frame(msg.data.data(), msg.data.size());

    // String comparison
    if (msg.encoding.size() == 4 &&
        memcmp(msg.encoding.data(), "rgb8", 4) == 0) { ... }
}
```

In `NROS_CPP_STD` mode, convenience conversions to `std::string` and
`std::vector` are available for users who need owned copies:

```cpp
std::string enc(msg.encoding.data(), msg.encoding.size());
```

C++20 users who want `std::span` can convert trivially:

```cpp
std::span<const uint8_t> s{msg.data.data(), msg.data.size()};
```

#### Raw message API

The raw (untyped) subscription API is unchanged — the callback
receives `(const uint8_t* data, size_t len)`, a pointer into the
arena's read slot. It is already borrowed by nature:

```rust
// Rust raw API
executor.add_subscription_raw("/topic", qos, |data: &[u8]| { ... });
```

```c
// C raw API
void callback(const uint8_t* data, size_t len, void* ctx) { ... }
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

- [x] 73.1 — Fix FreeRTOS `z_realloc` (returns NULL)
- [x] 73.2 — Fix ThreadX missing Rust `GlobalAlloc`
- [x] 73.3 — Slab fast-path in `zpico-alloc`
- [x] 73.4 — Triple buffer primitive
- [x] 73.5 — SPSC ring buffer primitive
- [x] 73.6 — Arena-based buffer allocation for subscriptions
- [x] 73.7 — Zenoh shim direct-write into triple buffer
- [ ] 73.8 — Borrowed message codegen (Rust)
- [ ] 73.9 — Borrowed message codegen (C/C++) and `nros::Span` header
- [ ] 73.10 — Executor zero-copy dispatch path
- [ ] 73.11 — DDS and XRCE-DDS shim integration
- [ ] 73.12 — Remove `SUBSCRIBER_BUFFERS` static array
- [ ] 73.13 — Remove `alloc` from zero-copy subscriber
- [ ] 73.14 — Remove `alloc` from timer callbacks
- [ ] 73.15 — Remove `alloc` from large service replies
- [ ] 73.16 — Remove `alloc` from zenoh ID formatting and executor config
- [ ] 73.17 — Document sizing and migration

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

### 73.12 — Remove `SUBSCRIBER_BUFFERS` static array

Once all subscription paths use arena-based buffers (73.6–73.11), the
static `SUBSCRIBER_BUFFERS` array in the zenoh shim can be removed.
This eliminates the dominant source of static memory waste
(128 × 1064 B = 133 KB with default config).

The `ZPICO_MAX_SUBSCRIBERS` build-time constant is no longer needed
for buffer pre-allocation — subscriber count is bounded only by the
executor's `MAX_CBS` and arena size.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`

### 73.13 — Remove `alloc` from zero-copy subscriber

The current `unstable-zenoh-api` zero-copy subscriber uses
`Box<dyn FnMut(&[u8], Option<MessageInfo>) + Send>` to store the user
callback as a heap-allocated trait object.

Phase 73's triple buffer + borrowed deserialization (73.4–73.10)
replaces this entirely. The new zero-copy path uses the same
executor callback mechanism as regular subscriptions (monomorphized
function pointer in `CallbackMeta`, concrete closure in arena entry)
— no `Box<dyn>` needed.

Once 73.10 is complete, remove:
- `ZeroCopyCallbackBox` type alias
- `ZenohZeroCopySubscriber` struct and its `alloc`-gated impl
- The `#[cfg(all(feature = "unstable-zenoh-api", feature = "alloc"))]`
  gates — the `unstable-zenoh-api` feature should work without `alloc`

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`

### 73.14 — Remove `alloc` from timer callbacks

`Timer` has dual callback storage: `callback_fn: Option<TimerCallbackFn>`
(bare function pointer, always available) and `callback_box:
Option<Box<dyn FnMut()>>` (heap trait object, `alloc`-gated).

The executor arena already stores timer callbacks as monomorphized
concrete closures (`TimerEntry<F>` where `F: FnMut()`). The `Box<dyn>`
path in `Timer` is redundant — it exists for a standalone `Timer`
usage pattern that the arena-based executor doesn't use.

Remove:
- `TimerCallback` type alias (`Box<dyn FnMut() + Send>`)
- `callback_box` field from `Timer` struct
- `new_with_box()` and `set_callback_box()` methods
- All `#[cfg(feature = "alloc")]` gates in `timer.rs`

Retain `TimerCallbackFn` (bare function pointer) for the C API path.

**Files**:
- `packages/core/nros-node/src/timer.rs`

### 73.15 — Remove `alloc` from large service replies

`handle_request_boxed()` in `nros-rmw` and `nros-node` returns
`Box<Reply>` for service responses too large for the stack. This is
used only by parameter services (which retain `alloc` as a hard
dependency).

For all other service types, `handle_request()` (stack-based) is
sufficient — ROS 2 service replies are typically small (< 1 KB).

Remove `handle_request_boxed()` from the public API surface. If
parameter services need it internally, keep it as a private method
gated on `param-services` (which already implies `alloc`).

**Files**:
- `packages/core/nros-rmw/src/traits.rs`
- `packages/core/nros-node/src/executor/handles.rs`

### 73.16 — Remove `alloc` from zenoh ID formatting and executor config

Two minor `alloc` usages:

1. **Zenoh ID hex string** (`zpico.rs:169`): `to_hex_string()` uses
   `alloc::format!()`. Replace with a method that writes into a
   caller-provided `heapless::String<32>` (16 hex bytes = 32 chars).

2. **Executor config string leak** (`types.rs:238`): `Box::leak()` to
   convert env var `String` to `&'static str`. This is already gated
   on `std` (env vars only exist on hosted platforms). Move the gate
   from `alloc + std` to just `std` — on `std` targets, the standard
   allocator is always available, so the explicit `alloc` gate is
   redundant.

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/zpico.rs`
- `packages/core/nros-node/src/executor/types.rs`

### 73.17 — Document sizing and migration

Update the embedded tuning guide with the new memory model:

- Arena sizing formula: `sum over subscriptions of
  (entry_overhead + slot_count × buf_size)`
- Default configuration memory footprint comparison (before/after)
- Migration guide from static `SUBSCRIBER_BUFFERS` to arena buffers
- QoS depth recommendations per use case

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
- [ ] Borrowed message types generated for Rust, C, and C++
- [ ] `nros::Span` and `nros::StringView` work on C++14 freestanding
      (GCC 5+, Clang 3.5+, no STL required)
- [ ] `SUBSCRIBER_BUFFERS` static array removed; memory usage reduced
- [ ] All existing tests pass (no regressions in copy-based path)
- [ ] `alloc` feature only required by `param-services`; all other
      nros functionality works without `alloc`
- [ ] `unstable-zenoh-api` zero-copy works without `alloc`
- [ ] `grep -r 'feature.*alloc' packages/core/ packages/zpico/nros-rmw-zenoh/`
      shows only `param-services`-related gates and `extern crate alloc`
      declarations
- [ ] Arena sizing documented with before/after comparison

## Notes

- **Single type, not two variants**: Messages with all fixed-size
  fields (Int32, Vector3, Quaternion) have no lifetime and are
  unchanged from today. Messages with unbounded strings/sequences
  gain a lifetime parameter (`Image<'a>`). There is no separate
  `ImageRef` / `ImageOwned` split — one type works for both
  publishing (borrows from user data) and receiving (borrows from
  transport buffer).
- Messages with lifetime parameters cannot be stored beyond the
  callback scope. Users who need to keep a received `Image<'a>`
  must copy the borrowed fields into owned storage manually.
- The copy-based receive path is **not removed**. Messages without
  lifetime parameters (all-fixed-size) work exactly as today. The
  buffer strategy (triple buffer / SPSC ring) is transparent —
  existing `add_subscription` calls work unchanged with the new
  arena allocation.
- **C/C++ message types use pointer+length pairs**, not Rust
  lifetimes. The "lifetime" is the callback scope — same contract
  as today's raw API. C++ uses freestanding `nros::Span<T>` and
  `nros::StringView` (C++14, no STL). No C++17 or C++20 required.
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
