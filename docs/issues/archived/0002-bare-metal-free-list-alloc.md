---
id: 2
title: Zenoh-pico free list allocator on bare-metal
status: resolved
type: bug
area: memory
related: []
resolved_in: zpico-alloc
---

All four bare-metal platform crates now share a single free-list allocator
via the `zpico-alloc` crate (`packages/zpico/zpico-alloc/`). This replaced
the broken bump allocators on ESP32 / ESP32-QEMU / STM32F4 (no-op `z_free`,
data-losing `z_realloc`) with the proven MPS2-AN385 first-fit free-list with
address-ordered coalescing. Each platform's `memory.rs` is now a thin
wrapper instantiating `FreeListHeap<N>` with its heap size (32–128 KB).
`zpico-alloc` has an optional `stats` feature for heap-usage tracking.
Remaining (not bugs): fixed heap size, first-fit fragmentation over very
long sessions.
