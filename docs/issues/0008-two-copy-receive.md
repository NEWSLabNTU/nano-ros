---
id: 8
title: Two-copy receive path and static buffer pre-allocation at scale
status: open
type: tech-debt
area: rmw
related: [issue-0007]
---

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

| Buffer                 | Per-unit                                            | Count                        | Default total |
|------------------------|-----------------------------------------------------|------------------------------|---------------|
| `SUBSCRIBER_BUFFERS`   | depth-4 SPSC ring of `SUBSCRIBER_BUFFER_SIZE` (default 1024) ≈ 4 KB | `ZPICO_MAX_SUBSCRIBERS` (8)  | **~32 KB**    |
| Executor arena entries | ~2304 B                                             | `NROS_EXECUTOR_MAX_CBS` (4)  | **~10 KB**    |

Default total static pre-allocation is therefore ≈ **~36 KB**. The dominant
cost is `SUBSCRIBER_BUFFERS`: 8 slots × a depth-4 ring of buffers, all
pre-allocated as a static array regardless of how many subscribers exist.
The default `ZPICO_MAX_SUBSCRIBERS` / `ZPICO_MAX_QUERYABLES` is **8** and is
env-configurable (`packages/zpico/nros-zpico-build/src/runner.rs:31`).

**Scaling problem**: If the buffer size is increased for large messages
(e.g., `ZPICO_SUBSCRIBER_BUFFER_SIZE=65536` for 64 KB compressed images),
the static array becomes 8 slots × depth-4 ring × 64 KB = **2 MB** —
impossible on any MCU. Reducing `ZPICO_MAX_SUBSCRIBERS` helps, but then the
system supports very few concurrent subscribers.

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

2. **Borrowed deserialization** (issue #7): The message struct borrows
   `&'a [u8]` slices from the subscriber buffer for variable-length
   fields, avoiding the CDR copy into `heapless::Vec`.

3. **Reduce subscriber slot count**: size `ZPICO_MAX_SUBSCRIBERS` to the
   actual number of subscribers. This is already configurable.

Combined with issue #7's borrowed deserialization, this gives a
zero-copy path from network to user callback for the payload data,
with only fixed-size header fields deserialized onto the stack.

**Progress — partial zero-copy paths landed**:

- The zero-copy *borrow* path landed as the opt-in **`lending`** feature
  (`SlotBorrowing` / `ZenohView`, `subscriber.rs:1143`). It eliminates
  copy #1 (the arena memcpy). The borrowed-deserialize step that would
  eliminate copy #2 is still pending — it links to issue #7.
- The alloc-requiring **`unstable-zenoh-api`** path skips
  `SUBSCRIBER_BUFFERS` entirely — the callback receives `&[u8]` pointing
  into zenoh-pico's internal buffer. However, it requires `alloc`
  (boxed callback closure) and bypasses the executor's LET semantics,
  making it unsuitable for alloc-free bare-metal use.

**Still open**: the DEFAULT receive path is two-copy, and the static
`SUBSCRIBER_BUFFERS` arrays are unconditionally pre-allocated regardless
of how many subscribers a node actually creates.

**Implementation note (where the copy-#1 fix lands).** The base
`Subscriber` trait already has a zero-copy in-place method
(`process_raw_in_place(f: impl FnOnce(&[u8]))` — borrows the head ring slot,
no arena memcpy), and some handle paths use it (`executor/handles.rs`). But
the **main executor arena dispatch** (`packages/core/nros-node/src/executor/
arena.rs`) still copies the slot into the per-subscriber `entry.buffer` via
the copying `try_recv_raw`/`n(&mut entry.buffer)`, then deserializes from
there. So design-direction item 1 (skip the arena buffer) is concretely:
route arena subscriber dispatch through the in-place borrow and deserialize
directly from the ring slot, which also lets the per-subscriber `entry.buffer`
shrink/disappear. **Caveat:** that lives in `nros-node`'s executor — an
actively-churning Phase 228 area (callback-group filter, tier resolver,
per-tier spawn) — so this change should be coordinated with / sequenced
after that work to avoid conflict. Copy #2 cannot be removed without the
borrowed-deserialization codegen of issue #7.

**Workarounds available today**:

- Set `ZPICO_MAX_SUBSCRIBERS` to the actual subscriber count to reduce
  static memory waste.
- Increase `ZPICO_SUBSCRIBER_BUFFER_SIZE` only when large messages are
  needed, accepting the memory tradeoff.
- Use the raw CDR API (`try_recv_raw`) with a caller-provided buffer
  to bypass the static buffer system entirely.
- Enable the `lending` feature to eliminate copy #1 on the receive path.
