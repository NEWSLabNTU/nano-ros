---
id: 7
title: Unbounded message sequences capped at 64 elements
status: open
type: enhancement
area: codegen
related: [rfc-0033, phase-229]
---

Generated message bindings use `heapless::Vec<T, N>` for unbounded sequences
(`uint8[] data`, `float32[] ranges`, etc.). The capacity `N` is hardcoded in
the codegen at **64 elements** (`NROS_DEFAULT_SEQUENCE_CAPACITY` in
`packages/cli/rosidl-codegen/src/types.rs`, line 490). The C/C++ generators
mirror this default (`C_DEFAULT_SEQUENCE_CAPACITY` at line 970,
`CPP_DEFAULT_SEQUENCE_CAPACITY` at line 1125, both `= 64`). It is a plain
compile-time `const` with no environment variable or attribute override.

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

Bounded sequences (`uint8[<=100] data`, i.e. `[<=N]`) are handled distinctly
— they use the declared bound as the capacity and do not suffer from the
default-capacity problem.

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

**Progress — Phase 229 (config + heap mode landed; borrowed pending)**:

The hardcoded-64 limitation is **resolved**; only the zero-copy `borrowed`
design direction above is still pending. As of `5aa64a233`:

- **Per-field capacity config** (229.1–229.4, all three langs): the `64`
  default is now a *default*, not a hardcoded ceiling. `nros-codegen.toml`
  sets per-field `cap = N` (e.g. `"sensor_msgs/LaserScan.ranges" = { cap = 1080 }`).
  `CapacityResolver` + discovery in `packages/cli/rosidl-codegen/src/config.rs`.
- **`heap` storage mode** (229.5, Rust + C + C++; sequences incl. of-strings
  and of-nested, plus strings): `mode = "heap"` emits a heap-backed container
  (`HeapSequence<T>` in C++, heap `Vec` in Rust) — large payloads carry **no
  inline stack/struct cost**. An `sensor_msgs/Image.data` at `{ cap = 921600,
  mode = "heap" }` is representable today on any target with a heap.
- **`borrowed` mode** (229.6, the zero-copy `&'a [u8]` design above): ⬜ **not
  landed** — design-of-record in RFC-0033. **This issue closes when 229.6
  lands** (per `docs/roadmap/phase-229-message-field-capacity-config.md`).

So large sensor messages are **usable now** via `mode = "heap"` (with a heap);
the remaining gap is the alloc-free zero-copy receive path for heapless
bare-metal, which is exactly 229.6 / issue #8's single-copy work.

**Workarounds available today**:

- Set `mode = "heap"` + an explicit `cap` in `nros-codegen.toml` for the
  large field (requires `alloc` on the target).
- Define bounded message types for the application's actual payload
  size (e.g., `uint8[<=4096] data` in a custom `.msg` file).
- Use raw CDR APIs (`try_recv_raw`) to access the receive buffer
  directly, bypassing the generated message types entirely.
