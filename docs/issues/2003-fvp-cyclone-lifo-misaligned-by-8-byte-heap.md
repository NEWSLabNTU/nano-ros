---
id: 2003
title: "The FVP workspace entry dies at boot: Cyclone's 16-byte lock-free LIFO gets an 8-aligned block from the Zephyr heap, and AArch64 `caspal` faults"
status: open
type: bug
area: [platform, rmw, zephyr, cyclonedds]
severity: high
found: 2026-10-23
related: [0232, 1324]
---

## Measured

`just zephyr verify-fvp-runtime`, 2026-10-23: the `demo_bringup:fvp` image
(realtime-cpp, two tiers, Cyclone) on FVP_BaseR_AEMv8R 11.31.28, Zephyr 3.7,
SDK 0.16.8. It became runnable only after this session's two fixes to that
lane: the recipes' missing `ZEPHYR_TOOLCHAIN_VARIANT` (issue 0698's rule) and
the resolver's working-directory dependence (phase-511 B2).

`fvp_ws_entry_two_tier_publishes` never sees `[telem] tick=` in 180 s.
Two runs, same failure. The guest output:

```
cyclonedds: session_create: dds_create_participant returned 1795737439
os: ELR_ELn: 0x000000000003ef10
os: ESR_ELn: 0x0000000096000021
os:   EC:  0x25 (Data Abort taken without a change in Exception level)
os: FAR_ELn: 0x0000000000272ae8
os: >>> ZEPHYR FATAL ERROR 0: CPU exception on CPU 1
...
os: FAR_ELn: 0x0000000000272be8
os: >>> ZEPHYR FATAL ERROR 0: CPU exception on CPU 2
```

Both tier threads fault at the same PC. Symbolized against the image's
`zephyr.elf`:

| | |
|---|---|
| PC `0x3ef10` | `ddsrt_atomic_casvoidp2` (`ddsrt/atomics/gcc.h:744`), instruction `caspal x4, x5, x6, x7, [x0]` |
| LR `0x3ef5c` | `ddsrt_atomic_lifo_push` (`ddsrt/src/atomics.c:173`) |
| FAR `0x272ae8` | inside `nros_platform::zephyr_heap::HEAP` (`0x172a28`, size `0x100b60`) |

ESR ISS `0x21` (DFSC) is an alignment fault. `caspal` is a 128-bit
compare-and-swap PAIR and requires a 16-byte-aligned address; `0x272ae8` and
`0x272be8` are 8-aligned and not 16-aligned.

## Cause (as far as traced)

`zpico-alloc`'s `FreeListHeap` is the arena behind `nros_zephyr_heap_alloc`,
which serves C `malloc` for the image (phase-391 W3, `60b4e0c1eb`). It
hard-codes `ALIGN = 8`, and its crate doc states the assumption:

> All allocations are 8-byte aligned. […] no nros runtime type needs more than
> 8-byte alignment.

That holds for nano-ros's own types and for 32-bit targets. It does not hold for
a C `malloc` on a 64-bit target, which owes `alignof(max_align_t)` (16 on
AArch64 and x86_64), nor for Cyclone in particular: `ddsrt_atomic_lifo_t` keeps
a `{pointer, aba}` pair updated with a double-width CAS. With LSE available, GCC
inlines that CAS as `caspal`, which faults on an 8-aligned block.

Not yet measured:

- Whether a 64-bit Zephyr Cyclone image on x86_64 (`native_sim/native/64`) is
  exposed. `cmpxchg16b` also needs 16-byte alignment, but GCC does not inline it
  without `-mcx16` and may route through libatomic instead.
- Which allocation is the LIFO's. Every allocation through this arena is
  8-aligned, so the fault occurs whenever the block lands on an address that is
  8- but not 16-aligned.

## Why nothing saw it

The only runtime test of a 64-bit Zephyr Cyclone image on hardware-like
emulation is `fvp_runtime_ws`, and no CI runner provisions the FVP. The lane was
also unrunnable on a CMake-4 host until this session fixed its toolchain
variant. Phase-511 B2 gave the test a `Tier::Gated` cell, and running that gated
lane found this.

## Fix direction (not applied)

Make the arena honour the target's `max_align_t` for the C funnel: 16 on 64-bit
targets, 8 on 32-bit. Either raise `ALIGN` per target, or take an alignment on
the C entry points and allocate at `max(8, align_of max_align_t)`.

The cost is padding on 64-bit images, which moves the heap sizing figures
issue 1324's work measured. Re-measure them on fixtures rebuilt after the
change. `allocations_are_8_byte_aligned` and the `GRANULARITY` arithmetic in
`zpico-alloc` change with it.

Acceptance: `just zephyr verify-fvp-runtime` prints `[telem] tick=` and
`[ctrl] tick=`. Add a host unit test in `zpico-alloc`: on a 64-bit target, every
C-funnel allocation is 16-aligned.
