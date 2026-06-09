# Phase 230 — Platform layer split: enforce the system-ABI boundary (alloc first)

**Goal:** Make the platform/RMW/core split (RFC-0034) a real, enforced
invariant. Today the `nros_platform_*` ABI is bypassed on every RTOS:
zenoh-pico and the Rust `#[global_allocator]` call `pvPortMalloc` /
`k_malloc` / `tx_byte_allocate` directly, so the platform layer's RTOS
providers are dead code. Route the **allocator** through the ABI on all
platforms first (the starter), then the other scalar services, dedupe the
per-RMW bridge, and add a lint that keeps the boundary from re-rotting.
The unified allocation funnel also yields the true heap stats that
[issue 0006](../issues/0006-rtos-dual-heap.md) needs.

**Status:** Planned

**Priority:** Medium — architecture/tech-debt. Design is locked (RFC-0034);
no new public API. Unblocks accurate embedded heap accounting and a
genuinely single system-access layer.

**Depends on:** RFC-0034 (this phase implements it), RFC-0006 (C-ABI
canonical + platform free-symbol model), [platform-c-abi.md](../../book/src/internals/platform-c-abi.md)
(symbol contract + drift gate). Touches the vendored zenoh-pico fork
(`packages/zpico/zpico-sys/zenoh-pico`) and `nros-zpico-build`.

## Overview

RFC-0034 establishes: (1) `nros_platform_*` is the sole system boundary;
(2) **scalar** services (alloc/sleep/clock/random) unify through it on
every platform, **opaque-struct** services (task/sync/net) stay
per-RTOS-vendored by design; (3) one platform-owned `z_* → nros_platform_*`
bridge, category-gated; (4) the platform port owns + inits the heap; (5) a
no-direct-kernel-call lint enforces it.

This phase delivers that in waves, alloc first. Each wave is independently
landable and leaves the tree green.

## Architecture

- **Bridge ownership:** the `z_* → nros_platform_*` alias TU becomes a
  single platform-layer artifact consumed by both `zpico-sys` and
  `nros-rmw-xrce` (retiring the duplicate `platform_aliases.c`). Emission
  is per-category: a **memory-only** alias on FreeRTOS/ThreadX/Zephyr
  (vendor keeps task/net), full alias where it already works
  (POSIX/bare-metal).
- **Vendor strip:** zenoh-pico's strong scalar defs (`z_malloc`/`z_free`/
  `z_realloc` and later `z_sleep_*`/`z_clock_*`) are guarded behind a fork
  `#ifdef` (`Z_FEATURE_NROS_PLATFORM_ALLOC` / `…_SCALAR`) so the alias wins
  with no double-definition.
- **Heap ownership:** each port owns + initializes its pool before first
  alloc; Rust global allocators (`nros-c`/`nros-cpp`) call
  `nros_platform_alloc`, not the raw kernel API.
- **Stats:** instrument the single `nros_platform_alloc` funnel
  (`used`/`peak`), superseding the Rust-only `nros_heap_used_bytes`
  accounting with a true C+Rust total.

## Work items

### Wave 0 — Audit + lint scaffold

#### 230.0.1 — Direct-kernel-call audit
Enumerate every direct host-allocator call outside the platform ports:
zenoh-pico vendored `system/<rtos>/system.c`, `nros-c`/`nros-cpp` global
allocators, `nros-rmw-xrce` aliases. Produce the authoritative bypass list
(seed: RFC-0034 table). Record each call site + intended `nros_platform_*`
target.

#### 230.0.2 — `no-direct-kernel-call` lint (alloc subset)
Extend `scripts/check-platform-abi-mirror.sh` (or a sibling
`check-no-direct-kernel-alloc.sh`) to fail on
`pvPortMalloc`/`vPortFree`/`k_malloc`/`k_free`/`tx_byte_allocate`/
`tx_byte_release`/`heap_caps_malloc` references outside the legitimate port
crate. Start **advisory** (warn) so Wave 1 can flip it to hard-fail once
the call sites are migrated. Wire into `just check`.

### Wave 1 — Allocator unification (the starter)

#### 230.1.1 — Fork guard for vendored scalar alloc
Guard `z_malloc`/`z_free`/`z_realloc` in zenoh-pico's
`system/{freertos,threadx,zephyr}/system.c` behind
`Z_FEATURE_NROS_PLATFORM_ALLOC`. Commit on the fork branch with linear
history; bump the submodule pointer per the vendored-fork workflow (agent
leaves the branch ready; maintainer pushes the fork).

#### 230.1.2 — Memory-only alias emission on RTOS
Add a `NROS_ZP_ALIAS_MEMORY_ONLY` path to the alias TU + `nros-zpico-build`
so FreeRTOS/ThreadX/Zephyr emit the scalar (`z_malloc`→`nros_platform_alloc`)
forwarders while leaving task/net to the vendor. Define
`Z_FEATURE_NROS_PLATFORM_ALLOC` for those targets. Remove the ineffective
ThreadX weak-`z_malloc` footgun (`nros-platform-threadx/src/platform.c`).

#### 230.1.3 — Zephyr scalar port surface
Stand up the scalar `nros_platform_alloc/dealloc/realloc` provider for
Zephyr (k_heap-backed) — today Zephyr has no C `nros_platform_*` provider
on the link path. Wire it so the memory-only alias resolves.

#### 230.1.4 — Rust global allocators through the ABI
Repoint `FreeRtosAllocator` / `ZephyrAllocator` / `ThreadXAllocator`
(`nros-c`/`nros-cpp/src/lib.rs`) at `nros_platform_alloc`/`_dealloc`
instead of the raw kernel calls. One funnel for C + Rust.

#### 230.1.5 — Init-order contract
Ensure each port initializes its pool before first alloc; document the
contract in [platform-c-abi.md](../../book/src/internals/platform-c-abi.md)
(board/runtime platform-init → transport/alloc). Verify on
ThreadX/FreeRTOS QEMU + Zephyr native_sim.

#### 230.1.6 — Unified heap stats
Instrument `nros_platform_alloc` (`used`/`peak`, opt-in `alloc-stats`) as
the true C+Rust total; keep `nros_heap_used_bytes()` as the public
accessor but back it with the funnel counter. Update + close
[issue 0006](../issues/0006-rtos-dual-heap.md).

#### 230.1.7 — Flip the lint to hard-fail
Once 230.1.1–230.1.4 land, make 230.0.2 a hard error for the alloc subset.

### Wave 2 — Remaining scalar services

#### 230.2.1 — sleep / clock / yield / random
Apply the Wave-1 pattern to the other scalar services (no struct ABI):
guard vendored defs, alias to `nros_platform_*`, extend the lint. Lower
risk than alloc (no heap-ownership/init subtlety).

### Wave 3 — Bridge dedup + boundary documentation

#### 230.3.1 — One platform-owned bridge
Collapse the duplicated `platform_aliases.c` (zpico-sys + nros-rmw-xrce)
into a single platform-layer shim both RMWs consume.

#### 230.3.2 — Document the opaque-struct boundary
Record in [platform-c-abi.md] (and ARCHITECTURE.md when RFC-0034 → Stable)
that task/sync/net stay per-RTOS-vendored by ABI constraint — a design
boundary, not debt — with the canonical-layout + `size_probe` static-assert
escape hatch noted for any future move (net first candidate).

## Out of scope

- Unifying opaque-struct services (task/mutex/condvar/socket) — RFC-0034
  D2; needs canonical layouts + static-asserts, deferred.
- Runtime platform pluggability — one port per binary stays (RFC-0006).
- Touching the working POSIX/bare-metal alias path beyond the dedup.

## Done when

- zenoh-pico + Rust allocations on FreeRTOS/ThreadX/Zephyr resolve to
  `nros_platform_alloc`; no direct kernel-allocator calls remain outside
  the ports (lint hard-fails on violation).
- `nros_heap_used_bytes()` reports a true C+Rust total on RTOS; [issue 0006]
  closed.
- The ThreadX weak-`z_malloc` footgun and the dead `nros-platform-*` alloc
  paths are gone.
- All embedded E2E (ThreadX/FreeRTOS QEMU, Zephyr native_sim, NuttX) stay
  green across the migration.
