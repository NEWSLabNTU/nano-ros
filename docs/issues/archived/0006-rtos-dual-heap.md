---
id: 6
title: Two separate heap allocators on RTOS platforms
status: resolved
type: tech-debt
area: memory
related: [rfc-0034, phase-230]
resolved_in: "Phase 230 Wave 1b (RFC-0034 D7)"
---

> **Resolved (2026-06, phase-230 Wave 1b).** The actionable gap — no way to
> see the true unified heap total on RTOS — is closed. A canonical platform
> ABI query `nros_platform_heap_used_bytes()` / `_total_bytes()`
> (`<nros/platform.h>`) reports it per port: Zephyr `sys_heap` (verified
> `used=8792 total=64896` on native_sim), ThreadX `tx_byte_pool_info_get`,
> POSIX `mallinfo2`, FreeRTOS `xPortGetFreeHeapSize`, esp `heap_caps_*`.
> Because the C side (zenoh-pico) funnels through `nros_platform_alloc` and
> — where nano-ros owns the Rust allocator — the Rust side too, the figure
> is exact (Mode A); where a framework owns the Rust allocator it shares the
> one kernel heap, so the native query is still exact (Mode B). The
> two-allocators-coexist structure itself is **by design** (RFC-0034: the
> platform owns one heap; the global allocator is an optional singleton) —
> not debt. Follow-ups (don't block this close): routing the convenience
> `nros_heap_used_bytes()` accessor through the platform query (phase-230
> 1b.3), the FreeRTOS C-side funnel (1c), real bare-metal `FreeListHeap`
> stats. Original analysis below.

> **Direction (2026-06):** addressed by the RFC-0034 platform layer split,
> implemented in [phase-230](../roadmap/phase-230-platform-layer-split.md)
> Wave 1. zenoh-pico's C allocations route through `nros_platform_alloc`
> (the C-side funnel). The Rust `#[global_allocator]` is an optional,
> board-selected singleton (RFC-0034 D6): where nano-ros owns it, the
> `nros_platform_alloc` funnel is the **exact** C+Rust total; where a
> framework owns it (Zephyr zephyr-lang-rust, esp-hal), the unified total
> comes from the **platform-native heap query** (Zephyr `sys_heap`,
> FreeRTOS `xPortGetFreeHeapSize`) since both share one kernel heap (D7).
> Two-mode but always exact. Closes when phase-230 Wave 1 lands.


On RTOS platforms (FreeRTOS, ThreadX), there are **two independent heap
allocators** that cannot share memory or statistics:

| Allocator                      | Who calls it                                                              | Backed by                                                                      |
|--------------------------------|---------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| zenoh-pico `z_malloc`/`z_free` | zenoh-pico C code (session state, buffers, hashmap buckets, vec growth)   | RTOS allocator (e.g. `pvPortMalloc`, `tx_byte_allocate`)                       |
| Rust `#[global_allocator]`     | nros Rust crates when `alloc` feature is enabled (`Box`, `Vec`, `String`) | RTOS allocator on FreeRTOS (via `FreeRtosAllocator`); see table below          |

**Current state by platform**:

| Platform   | z_malloc backend                                                   | Rust global_allocator                                                  | nros alloc feature   |
|------------|--------------------------------------------------------------------|------------------------------------------------------------------------|----------------------|
| Bare-metal | `zpico-alloc` (static free-list, 32–128 KB)                        | None                                                                   | Disabled             |
| FreeRTOS   | `pvPortMalloc` (C, in zenoh-pico `system/freertos/system.c`)       | `FreeRtosAllocator` → `pvPortMalloc` (in `nros-c/src/lib.rs`)          | Disabled in examples |
| ThreadX    | `tx_byte_allocate` (C, in `packages/core/nros-platform-threadx/src/platform.c`) | `ThreadXAllocator` (nros-c / nros-cpp `src/lib.rs`)       | Disabled             |
| NuttX      | libc `malloc` (C, via POSIX `system/unix/system.c`)                | Standard Rust allocator (libc `malloc`)                                | Enabled (`std`)      |
| Zephyr     | `k_malloc` (C, in zenoh-pico `system/zephyr/system.c`)             | `ZephyrAllocator` (k_malloc/k_free)                                    | Varies               |

**Concerns**:

1. ~~**FreeRTOS `z_realloc` returns NULL**~~ (Fixed) — implemented as
   alloc-copy-free in `system/freertos/system.c`, matching ThreadX.

2. ~~**ThreadX has no Rust global allocator**~~ (Fixed) — added
   `ThreadXAllocator` in both `nros-c/src/lib.rs` and `nros-cpp/src/lib.rs`,
   wrapping `z_malloc`/`z_free` (which delegate to `tx_byte_allocate`/
   `tx_byte_release`). Gated on `alloc + !std + platform-threadx`.

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

**Possible improvements** — both landed (opt-in, non-breaking; commit `a16c824f4`):

- ~~Implement `GlobalAlloc` on `FreeListHeap`~~ **(done)** — behind the
  opt-in `zpico-alloc/global-alloc` feature, so a bare-metal board can
  install the same free-list heap as the Rust `#[global_allocator]`,
  unifying the C (`z_malloc`) and Rust heaps. Backing storage is now
  8-byte aligned for soundness. Default builds install no global allocator
  (unchanged). Demonstrated in `nros-platform-mps2-an385` (`global-alloc`).
- ~~Extend heap usage tracking to RTOS platforms~~ **(partially done)** —
  opt-in `alloc-stats` `used`/`peak` tracking on the FreeRTOS/ThreadX/Zephyr
  Rust global allocators (reusable `zpico_alloc::HeapStats`), exposed as
  `nros_heap_used_bytes()` / `nros_heap_peak_bytes()`.

**Remaining (why this stays open):** the unified heap + stats are *opt-in*,
not default, and the RTOS stats count only the Rust `#[global_allocator]`
footprint — zenoh-pico's direct C-side `z_malloc`/`pvPortMalloc` traffic is
not included. For the true unified total, instrument the C allocator
(`nros_platform_alloc`) or use the RTOS-native query (FreeRTOS
`xPortGetFreeHeapSize()`, ThreadX `tx_byte_pool_info_get()`).
