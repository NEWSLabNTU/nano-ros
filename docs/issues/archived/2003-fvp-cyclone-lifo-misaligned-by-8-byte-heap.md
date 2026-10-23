---
id: 2003
title: "The FVP workspace entry dies at boot: Cyclone's 16-byte lock-free LIFO gets an 8-aligned block from the Zephyr heap, and AArch64 `caspal` faults"
status: resolved
type: bug
area: [platform, rmw, zephyr, cyclonedds]
severity: high
found: 2026-10-23
resolved: 2026-10-23
related: [0232, 1324, phase-511]
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

## Which allocation it was

Not the TLSF path. rlsf rounds its pool start up to `GRANULARITY` (32 on
64-bit) and places each payload `GRANULARITY / 2` past a block start, so its
blocks were already 16-aligned on 64-bit. Both fault addresses lie in the
512-byte SLAB, which follows the main arena inside the `HEAP` static
(`0x272ae8 - 0x172a28 = 0x1000c0`, just past a 1 MiB arena) and are 0x100
apart, i.e. four 64-byte slots. The slab hands out `base + k * 64` with no
rounding, and its storage was `#[repr(C, align(8))]`, so its alignment was
whatever the linker gave the static: here 8 mod 16. The LIFO's owner is a
Cyclone allocation of 64 bytes or less.

## Fix (2026-10-23)

`zpico-alloc`:

- `ALIGN` is `rlsf::GRANULARITY / 2`, i.e. `2 * size_of::<usize>()`: 16 on
  64-bit, 8 on 32-bit. It is now public and asserted at compile time.
- The backing storage is `align(16)`, with a compile-time assert that it is at
  least `ALIGN` and that the slab slot size is a multiple of it.

No sizing figure moves. `ALIGN` equals rlsf's payload alignment, so
`max_overhead` (`align - GRANULARITY / 2 + header`) is unchanged and no request
pays padding. The slab's slot size was already a multiple of 16. The only cost
is up to 8 bytes of padding in each `HEAP` static, once. `tests/heap_replay.rs`,
which replays a recorded Zephyr allocation trace, is unchanged and passes.

On a 32-bit target the guarantee stays 8. That is ARM's `max_align_t`.
riscv32's is 16, but nothing the arena serves there needs more than 8.

Test: `every_block_is_align_aligned_wherever_the_heap_sits` allocates every
slab slot and four TLSF sizes from a heap at two placements, 8 bytes apart, so
one of them starts 8 mod 16 if the storage is only 8-aligned. With the storage
reverted to `align(8)` it fails (`block 0x…1d68 is not 16-aligned`). The test
it replaced checked one placement, and passed by luck.

Acceptance: `just zephyr verify-fvp-runtime` on a rebuilt image:
- `fvp_runtime_ws`: 2/2 pass, including `fvp_ws_entry_two_tier_publishes`.
- `fvp_smoke`: pass.
- The rebuilt image places `HEAP` at `0x144150`.
