---
rfc: 0034
title: "Platform Layer Split & System-ABI Ownership"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-230]
supersedes: []
superseded-by: null
---

# RFC-0034 — Platform Layer Split & System-ABI Ownership

## Summary

The platform layer owns **all** OS/kernel access (heap, time, sleep,
threading, sync, network, timer) behind a single canonical C ABI
(`nros_platform_*`, defined in RFC-0006 + [platform-c-abi.md]). Core and
RMW layers depend on that ABI and **never** touch the host kernel
directly. Today that boundary holds on POSIX and bare-metal but is
**bypassed on every RTOS** (FreeRTOS/ThreadX/Zephyr): vendored zenoh-pico
and the Rust `#[global_allocator]` call `pvPortMalloc` / `k_malloc` /
`tx_byte_allocate` directly, so the platform ABI's RTOS providers are dead
code. This RFC makes the boundary an enforced invariant, classifies which
services can be unified (scalar) vs which are constrained by opaque-struct
ABI (threads/sync/net), and mandates the **allocator** as the first
service to unify — which also yields the true unified heap stats that
[issue 0006] needs.

## Motivation / problem

Intended layering:

```
core (nros-node, runtime, codegen output)  ─┐
RMW  (zenoh-pico, xrce, cyclonedds, dust)   ─┼─► nros_platform_*  ─► one port per binary
                                             ┘     (link-time)        (posix/freertos/threadx/zephyr/…)
```

Reality (measured — see [issue 0006] + the Phase 230 audit):

| Service | POSIX / bare-metal | FreeRTOS | ThreadX | Zephyr |
|---|---|---|---|---|
| `z_malloc` (zenoh-pico) | → `nros_platform_alloc` (alias TU) | → `pvPortMalloc` **direct** | → `tx_byte_allocate` **direct** | → `k_malloc` **direct** |
| Rust `#[global_allocator]` | (host) | → `pvPortMalloc` **direct** | → `z_malloc` (vendor) | → `k_malloc` **direct** |

Concrete defects this creates:

- **Dead platform code.** `nros-platform-{freertos,threadx}`'s C
  `nros_platform_alloc` (and the would-be Zephyr equivalent) is never on
  the link path — a platform layer that exists but isn't used. False
  sense of a clean split.
- **A footgun.** `nros-platform-threadx` ships a `__attribute__((weak))
  z_malloc → nros_platform_alloc` that is *silently shadowed* by
  zenoh-pico's strong `z_malloc`. Looks bridged; isn't.
- **Duplicated bridge.** The `z_* → nros_platform_*` alias TU
  (`platform_aliases.c`) is copied per-RMW — once in `zpico-sys`, again in
  `nros-rmw-xrce`. The split should have *one* bridge the platform layer
  owns, both RMWs consume.
- **No unified heap accounting.** Because there is no single allocation
  funnel on RTOS, `nros_heap_used_bytes()` counts only the Rust side;
  zenoh-pico's C traffic is invisible ([issue 0006]'s open item).

The platform ABI itself is fine and mature (94 symbols, drift-gated, two
port shapes — RFC-0006). The gap is **enforcement**, not design.

## Design

### D1. The boundary is an invariant

`nros_platform_*` is the **sole** system boundary. No core crate, no RMW
crate (including vendored transport C), and no Rust `#[global_allocator]`
may call a host-kernel allocator/clock/thread/socket primitive directly.
Every such call resolves — at link time — to a `nros_platform_*` symbol
provided by exactly one port crate.

Consumers never depend on a *specific* port crate (no
`nros-rmw-zenoh → nros-platform-threadx` edge); they depend on the **ABI**
(the headers + the `extern "C"` mirror) and the final binary chooses the
port. This is already RFC-0006's link-time-resolution model; D1 just
forbids the bypass.

### D2. Service classification — scalar vs opaque-struct

The deciding constraint on *what can be unified* is ABI shape, not effort:

| Class | Services | ABI shape | Unify through `nros_platform_*`? |
|---|---|---|---|
| **Scalar** | alloc/free/realloc, sleep, clock, yield, random | `(scalar) → scalar` | **Yes, on every platform.** No struct layout to disagree on. |
| **Opaque-struct** | task, mutex, condvar, socket, endpoint | by-value `_z_*_t` whose layout is per-RTOS (`TX_THREAD*` embedded, `int fd`, …) | **Constrained.** A generic alias layout has a different by-value ABI than the vendor layout → pass-by-value mismatch → crashes (the documented ESP32/NuttX `lhu`-on-an-IP faults; see `nros-zpico-build/src/runner.rs` Phase 160 notes). |

**Rule:** scalar services MUST route through the platform ABI on all
platforms. Opaque-struct services MAY stay in the per-RTOS vendored
system layer; unifying them requires a *canonical* opaque layout plus a
`size_probe` `_Static_assert` (the pattern bare-metal/ThreadX net already
use), and is **explicitly out of scope** for the initial split. This is a
documented design boundary, not debt.

### D3. One bridge, platform-owned, category-gated

The `z_* → nros_platform_*` forwarder TU moves to a single
platform-layer artifact, consumed by both zenoh-pico and XRCE (retire the
duplicate `platform_aliases.c` copies). Emission is **per-category**:

- On platforms where the vendor system layer owns opaque services
  (FreeRTOS/ThreadX/Zephyr task+net), emit a **memory-only** (scalar)
  alias and leave task/net to the vendor — extends the existing
  `NROS_PLATFORM_ALIASES_SKIP_TASK` / net-gating precedent.
- The strong vendored scalar definitions (`z_malloc` etc.) are stripped
  behind a fork guard (e.g. `Z_FEATURE_NROS_PLATFORM_ALLOC`) so the alias
  wins instead of double-defining. The vendored zenoh-pico is already a
  fork carrying `ZENOH_*` accommodations; this is one more, scoped to the
  scalar functions.

### D4. Allocator ownership + init contract

The platform port **owns and initializes the heap/pool** (ThreadX
`tx_byte_pool`, FreeRTOS heap region, Zephyr `k_heap`/`sys_heap`,
bare-metal `FreeListHeap`) before the first allocation. Init order
becomes a contract: *board/runtime platform-init → then any Rust or RMW
allocation*. The Rust `#[global_allocator]` (`FreeRtos/Zephyr/ThreadX`
allocators in `nros-c`/`nros-cpp`) routes through `nros_platform_alloc`,
not the raw kernel call. With one funnel, heap stats instrument
`nros_platform_alloc` once and report the true C+Rust total — closing
[issue 0006] as a side effect rather than a separate feature.

### D5. Enforcement gate

Extend `scripts/check-platform-abi-mirror.sh` (or a sibling) with a
**no-direct-kernel-call** lint: fail the build if any non-port crate
references `pvPortMalloc`/`vPortFree`/`k_malloc`/`k_free`/`tx_byte_allocate`
/`tx_byte_release`/`heap_caps_malloc` (and, later, the thread/sync
primitives) outside the platform port that legitimately defines the
`nros_platform_*` symbol. This is what keeps the boundary from re-rotting.

## Decision

1. **Alloc first** (the starter): unify the allocator through
   `nros_platform_alloc` on all RTOSes; route Rust global allocators
   through it; instrument once → unified stats.
2. **Scalar services next** (sleep/clock/random): same treatment, low risk.
3. **Opaque-struct services scoped out**: documented as the per-RTOS
   boundary; revisit only behind a canonical-layout + static-assert effort.
4. **One bridge**: dedupe zenoh/XRCE alias TUs into a platform-owned shim.
5. **Gate it**: the D5 lint prevents regression.

Work breakdown: [phase-230].

## Open questions

- **Fork maintenance.** Stripping vendored scalar defs behind a guard adds
  rebase cost on upstream zenoh-pico bumps. Acceptable vs the alternative
  (RTOS-native pool *query* for stats only, no unification — [issue 0006]
  Option B)? The query path gets stats without touching the layer split,
  but leaves the bypass (and dead code) in place. This RFC chooses
  unification because the split is the stated goal; revisit if fork churn
  proves heavy.
- **Do opaque services ever move?** Net is the strongest candidate (a
  `nros_platform_net_*` surface already exists, 29 symbols). Threads/sync
  are the least likely (deep struct coupling). Left open.
- **Zephyr port crate.** `nros-platform-zephyr` *does* ship
  `nros_platform_alloc` (k_heap-backed) as a Zephyr CMake module; the gap
  is link-line presence + who installs the Rust global allocator (next
  point), not a missing port.

- **RTOS-owned Rust global allocator.** On platforms whose Rust
  integration provides its own `#[global_allocator]` (Zephyr via
  zephyr-lang-rust → `k_malloc`; potentially others), the Rust side does
  not route through `nros_platform_alloc` and nros does not own it. A true
  single funnel then requires the nros entry/board to **install its own
  `#[global_allocator]`** wrapping `nros_platform_alloc`, shadowing the
  RTOS module's. The alternative — route only the RMW C side through the
  ABI and read the true total from the RTOS-native heap query (Option B,
  e.g. Zephyr `CONFIG_SYS_HEAP_RUNTIME_STATS`) — gets correct stats without
  owning the Rust allocator, since both already share one kernel heap.
  Decide per platform; this gates the Zephyr Wave-1 slice ([phase-230]).

## Changelog

- 2026-06: Initial draft. Captures the split-enforcement decision, the
  scalar/opaque-struct ABI classification, and the alloc-first plan that
  subsumes [issue 0006].

## See also

- RFC-0006 [portable-rmw-platform-interface](0006-portable-rmw-platform-interface.md)
  — C-ABI-canonical meta-decision; platform = free `extern "C"` symbols,
  link-time resolution; L0/L1/L2 ladder.
- RFC-0001 [architecture-overview](0001-architecture-overview.md) — layer map.
- RFC-0003 [rtos-integration-pattern](0003-rtos-integration-pattern.md) —
  per-RTOS adapter pattern; §9 std/alloc-per-platform policy.
- [platform-c-abi.md](../../book/src/internals/platform-c-abi.md) — the
  symbol contract + port-authoring guide + drift gate.
- [issue 0006](../issues/0006-rtos-dual-heap.md) — the dual-heap / unified
  stats item this RFC's alloc unification resolves.
- [phase-230](../roadmap/phase-230-platform-layer-split.md) — work breakdown.

[platform-c-abi.md]: ../../book/src/internals/platform-c-abi.md
[issue 0006]: ../issues/0006-rtos-dual-heap.md
