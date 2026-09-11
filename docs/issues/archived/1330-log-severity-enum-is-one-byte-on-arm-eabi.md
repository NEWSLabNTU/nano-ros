---
id: 1330
title: "`nros_log_severity_t` is ONE byte on every ARM EABI target, so phase-417's
  own width guard turned a latent ABI mismatch into a hard build failure for every
  cross C/C++ image"
status: resolved
type: bug
area: c-api
related: [phase-417, phase-448, 1146, 0238]
resolved_in: phase-448-w3
---

## Problem

`bbc4fb4a2` (phase-417 stage 3, 2026-09-11) added a width guard to
`packages/api/nros-c/include/nros/log.h`:

```c
_Static_assert(sizeof(nros_log_severity_t) == sizeof(int),
               "nros_log_severity_t must be int-sized: the Rust mirror is "
               "repr(transparent) over c_int");
```

**It fires.** `arm-none-eabi-gcc` defaults to `-fshort-enums` (it is the AAPCS
default, not a flag anyone in this tree chose), and the largest enumerator is
`NROS_LOG_SEVERITY_FATAL = 50`, so the enum is packed to **one byte**. Measured
on the pinned `arm-none-eabi-gcc 13.2-nros4`:

```
$ printf 'typedef enum e { A=0, B=50 } e_t;\nchar p[sizeof(e_t)];\n' > e.c
$ arm-none-eabi-gcc -mthumb -march=armv7-m -c e.c && arm-none-eabi-nm -S e.o
00000000 00000001 B p          # 1 byte  (armv7-a: also 1)
$ arm-none-eabi-gcc -mthumb -march=armv7-m -fno-short-enums -c e.c && ...
00000000 00000004 B p          # 4 bytes
```

`nros-c`'s own build script compiles `c-stubs/log_fmt.c`, which includes the
header, with the cross compiler — so the failure is not confined to a lane that
uses logging. **Every cross C/C++ image fails to build**, at
`_cargo-build_nros_cpp` / `_cargo-build_nros_c`:

```
include/nros/log.h:83:1: error: static assertion failed: "nros_log_severity_t must be int-sized …"
error: failed to run custom build command for `nros-c v0.5.0`
```

Reproduced on all six `examples/mps2-an385-freertos/cpp/*` leaves via
`scripts/build/fixtures-build.sh freertos cpp zenoh`. The same default applies
to `armv7a-nuttx-eabihf`, which `packages/api/nros/src/sizes.rs` already records
in prose for the C++ QoS enums:

> These are `#[repr(C)]` fieldless enums, so their width follows the *target C
> ABI*: `c_int` (4 bytes) on x86_64, but **1 byte on ARM EABI**.

So the tree already knew. The guard is the first thing that asked.

## The guard is RIGHT — the mismatch was real and latent

This is not a bad assertion to relax. Two independent contracts require an
int-wide enum here, and both were being violated on ARM before the guard
existed:

1. **The Rust mirror.** `nros_log_severity_t` in `packages/api/nros-c/src/log.rs`
   is `#[repr(transparent)]` over `core::ffi::c_int`, deliberately — phase-417
   stage 3 changed it FROM `#[repr(u8)]` precisely because "one side passed a
   byte where the other passed four". On ARM the sides were swapped, not fixed.
2. **The header's own documented contract.** It promises the gaps between the
   rcutils levels are usable as thresholds, that "any `int` a C caller can
   produce here is defined", and `to_facade` is total over `c_int`. A one-byte
   enum cannot carry any `int`; values above 255 silently truncate.

It survived because AAPCS promotes sub-word arguments to 32-bit registers, so
the byte and the word happen to agree for the seven named values passed by
register. It is undefined either way, and it is wrong the moment a severity
reaches memory (a struct field, a varargs slot, an array).

## Why no lane caught it

No merge-gating lane builds FreeRTOS or NuttX — the CLAUDE.md note under issue
1115 says the same thing about the NuttX config snapshot, which failed from a
clean clone for two days for the same reason. `check-fast` and `test-unit` are
host-only; `check-c` / `check-cpp` compile for the host, where `int` is 4 bytes
and the assert holds.

## Resolution

Fixed in phase-448 W3 (this was the blocker for measuring the FreeRTOS C++
carrier's app-task stack — no embedded C++ image could be built).

The C enum is pinned to `int` width on every target with a width-forcing
enumerator:

```c
    NROS_LOG_SEVERITY_FORCE_INT_WIDTH_ = 0x7fffffff,
```

Chosen over the two alternatives:

- **`-fno-short-enums` in the build flags** would need every consumer, in tree
  and out, to pass it, and a TU that forgot would disagree about the width of a
  type in a shared header — which is worse than the bug, because it is silent.
  The fix has to live in the header.
- **Narrowing the Rust mirror to follow the target C ABI** (the answer
  `sizes.rs` uses for the C++ QoS enums) cannot work here: a `#[repr(C)]`
  fieldless enum holds only its declared variants, and this type's contract is
  that every `c_int` is representable. That is why phase-417 made it a
  `repr(transparent)` newtype in the first place.

The sentinel is not a level, is spelled with a trailing underscore so it reads
as reserved, and is `cbindgen:ignore`d on the Rust side the way the seven real
levels are. `to_facade` is total, so a caller that somehow passed it resolves to
`FATAL` like any other out-of-range value rather than being rejected.

Verified: `sizeof(nros_log_severity_t) == 4` under `-mthumb -march=armv7-m`
(short-enums default) and the six FreeRTOS C++ example images build and run.
