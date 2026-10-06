---
id: 1706
title: "The parameter store's 285,696-byte heap allocation is sized from the declaration only on Zephyr, and only when the bringup declares `param_services`"
status: open
type: tech-debt
area: [sizing, zephyr, freertos, threadx, cpp]
severity: low
found: 2026-10-06
related: [1702, 1677, 1529, 0756, phase-382]
---

## What

Issue 1702 made the Zephyr nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`) take its
DEFAULT from the declared capability axes. A bringup that declares
`param_services` gets 512 KiB, and one that declares `lifecycle` gets 192 KiB.
Both values are measured, and the carrier is the generated west application's
`CONFIG_NROS_CAPABILITY_*` assignment.

The allocation that motivated it can still happen in three places that
derivation does not reach:

1. **Other RTOS heaps.** `Executor::leak_parameter_storage` boxes one
   `ParameterStorage<MAX_PARAMETERS>`, which is 285,696 bytes at the default 32
   slots (each slot is ~8.5 KiB, sized by `ParameterValue`'s `StringArray`
   variant). On FreeRTOS that comes out of heap_4 (`configTOTAL_HEAP_SIZE`), on
   ThreadX out of the byte pool, and on NuttX out of the system heap. No in-tree
   image on those platforms declares `param_services`, so nothing has measured
   it there, and no default follows the declaration.
2. **C++ images that never declare the axis.** A C++ image always carries the
   store (`param-store`, issue 1529). Anything that declares a parameter builds
   it: a launch `<param>` seed, `declare_parameter`, or a `ComponentNode` that
   declares. On Zephyr such an image keeps the 64 KiB default and halts at the
   first declaration with `HEAP EXHAUSTED (TOO SMALL): request 285696`, which is
   1702's symptom without 1702's trigger.
3. **The store's size is not consulted.** A bringup whose contracts declare
   `params:` derives a far smaller store (phase-446 W4: 25 scalar slots measured
   at 4,200 B). It still gets the 512 KiB default. That over-sizes the heap, the
   safe direction, and stating the heap overrides it. It still costs RAM that
   nothing derived.

## Rejected direction (measured, issue 1702)

One option was to make the store a named `.bss` static. That reserves the
bytes in every image that LINKS the declare path, not only in those that run
it. A plain mps2 C++ talker gained 280,832 B of `.bss` for a store it never
builds.

## Fix direction

The sound end state is phase-382 W3', the store carved from caller-placed
storage, together with a store whose footprint follows the parameters actually
declared. Until then, give each RTOS heap knob the same capability-conditional
default Zephyr now has, measured on a real image per platform, and make the
derivation also fire for a C++ image whose launch file seeds a `<param>`.

## Progress (2026-10-06): the refusal is named on every RTOS, and a seed implies the store

Status stays **open**. What landed is listed first, then what is still missing.

### What landed

1. **The store's allocation is fallible, on every RTOS and in every
   language.** `Executor::leak_parameter_storage` used `Box::new_uninit`,
   which sends a refused request to the OOM handler. It now uses
   `nros_rmw::fallible::try_box_uninit` (issue 1551's rule, in the
   uninitialised form issue 0756 needs). A refusal is recorded once and not
   retried. `declare_parameter*` returns `false`, and
   `register_parameter_services` returns
   `Err(Transport(BadAlloc))`. The refusal is logged with the store's
   COMPILED size, `PARAMETER_STORE_BYTES = size_of::<ParameterStorage>()`.
   That size already folds the contract-derived `MAX_PARAMETERS` and the
   board capacities. There is no formula and no measured constant, so the
   reported number is always this image's own.
2. **FreeRTOS heap_4 names the request.** `nros_platform_alloc` reports a
   request that is certain to fail (`size > xPortGetFreeHeapSize()`) through
   the registered log writer, before heap_4 calls the argument-less
   `vApplicationMallocFailedHook`. This matches Zephyr's `HEAP EXHAUSTED`
   line (issue 1370).
3. **(b) is decided as an implied STORE, never implied services.** A node
   may declare parameters locally without the six services, so neither a
   configure error nor turning the services on is right. The image does still
   need the allocation. A launch `<param>` seeds the store through the same
   declare path whether or not `param_services` is declared
   (`shared/declare_calls.jinja`). So the generated west application now sets
   `CONFIG_NROS_PARAM_STORE`, and `NROS_ZEPHYR_HEAP_SIZE` takes 512 KiB for
   it as it does for `NROS_CAPABILITY_PARAM_SERVICES`. A store that only
   application code reaches cannot be seen at build time. Item 1 refuses it
   at run time, with the number.

### Measured (real images)

| platform / image | heap | result |
| --- | --- | --- |
| Zephyr native_sim, features `zephyr_cpp_params`, heap stated 131072, `NROS_HEAP_EXHAUSTION_IS_FATAL=n` | before | `HEAP EXHAUSTED … request 285696`, then **SIGSEGV** (rc 139, issue 0589's recursion) |
| same | after | the same line, then `parameter store refused: needs 285696 bytes in one allocation (32 slots x 8928)`; the image keeps running |
| same, boot-report fixture (`IS_FATAL=y`) | both | the platform halts inside the allocator and names `request 285696`. The allocator decides that case, and it was already loud |
| Zephyr, same image with `features = []` (launch seeds `publish_period_ms`; lifecycle still declared) | old derivation 196608 (stated) | `HEAP EXHAUSTED … request 285696, arena 197120`, then kernel panic |
| same | derived (this change) 524288, `CONFIG_NROS_PARAM_STORE=y` | boots and publishes the seeded `250` |
| FreeRTOS mps2-an385 (QEMU, live router), rust talker + `param_services` + one parameter | default 720,896 (cargo, zenoh) | `nros: heap peak 473256 of 720896` (163,472 without parameters). The store on armv7m is **280,832** B. It fits, with 247,640 free |
| same, `NROS_FREERTOS_HEAP_KB=400` | before | `*** MALLOC FAILED ***` only |
| same | after | `HEAP EXHAUSTED: request 280832 bytes, free 246320 of 409600 bytes -- raise NROS_FREERTOS_HEAP_KB (issue 1706)`, then the hook |
| ThreadX threadx-linux, rust talker + `param_services` + one parameter | 4,105,736 pool | `byte pool peak 201736` (181,144 without). The Rust store comes from the host allocator on this board, not the pool |

The FreeRTOS measurement found one trap. heap_4 reports 0 free until its
first `pvPortMalloc`, so the first boot allocation (56 B, from lwIP) read as an
exhaustion. The check skips `free == 0`.

Tests:
- `executor::tests::a_heap_too_small_for_the_parameter_store_refuses_the_declaration`
  uses a per-thread `cfg(test)` seam in place of a NULL from the heap, with an
  unrefused negative control. It was first written against a capped test
  `#[global_allocator]`: against the old code that version ABORTED the test
  process with `memory allocation of 285696 bytes failed`, and it passed
  after. It was replaced because `check-feature-contract` clause (e) allows
  only one global allocator.
- `fallible::tests::an_impossible_uninit_box_is_none_not_an_abort`.
- `west_app::tests::a_launch_param_seed_reaches_kconfig_without_the_axis`.

The FreeRTOS C report has no automated test. The port has no host test
harness, and its evidence is the before/after QEMU pair above.

### Still missing

- **No RTOS default follows the store's size.** Zephyr's is a flat 512 KiB
  per capability. FreeRTOS, ThreadX and NuttX have no capability term at all.
  The size is a compile-time fact of the Rust build, and every heap carrier is
  decided before or outside that build (Kconfig at configure, `FreeRTOSConfig.h`,
  `threadx_hooks.c`). Closing this needs the size at configure time. The likely
  route is a sizing-descriptor fact produced by a size probe (the
  per-build sizes-header mechanism already measures Rust layouts for C), so the
  carriers read it as they read `[params]`. It should not be a formula over
  `ParameterValue`'s enum layout.
- **FreeRTOS cmake DDS road.** The 640 KiB default cannot carry a store. By
  arithmetic from issue 1624's measured peak, 447,944 + 280,832 = 728,776 >
  655,360. Not run: no in-tree Cyclone FreeRTOS image declares parameters.
  Item 2 makes the failure name the request instead of only `MALLOC FAILED`.
- **rv-virt-threadx, NuttX and esp32 were not measured.**
  - rv-virt-threadx: by arithmetic its 4 MiB pool leaves 3,380,664 B free at
    the worst measured peak (`threadx_hooks.c`), so a 280 KiB store fits.
  - NuttX: its heap is the board's RAM.
  - esp32-c3 bare metal: its heap cannot hold a default-sized store at all.
    There, a contract `params:` declaration, which shrinks `MAX_PARAMETERS`,
    is the only way in.
- **On a Zephyr image with `NROS_HEAP_EXHAUSTION_IS_FATAL=y`, and on
  FreeRTOS, the allocator halts before the fallible path can return.** The
  number is still named, by the allocator. A pre-check would avoid the halt,
  but it needs to know that the arena is FIXED. `nros_platform_heap_total_bytes`
  is `mallinfo`'s current arena on POSIX and FreeRTOS heap_3, and that arena
  grows, so a pre-check there would refuse an image that works.
