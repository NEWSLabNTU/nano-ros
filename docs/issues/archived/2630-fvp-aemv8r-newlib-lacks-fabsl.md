---
id: 2630
title: "fvp-aemv8r-smp with newlib declares no fabsl, so a C++ image reaching Eigen's long-double abs fails to compile"
status: resolved
type: bug
severity: low
area: [zephyr, build]
related: [phase-524]
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "phase-524 W3"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md) W3** (board
descriptors). GitHub #4017. Consumer: ASI's C++ image on `fvp-aemv8r-smp`.

## What happens

ASI's image selects newlib (`CONFIG_NEWLIB_LIBC=y`, `CONFIG_GLIBCXX_LIBCPP=y`).
Its vendored Autoware controller code includes Eigen, and Eigen's
`numext::absdiff<long double>` calls `fabsl` unqualified. Every such TU stops:

    Eigen/src/Core/MathFunctions.h:1318:10: error: 'fabsl' was not declared in this scope; did you mean 'fabsf'?

newlib declares its `*l` math functions only under `_LDBL_EQ_DBL` (long double
the same as double). On aarch64 long double is IEEE binary128, so the Zephyr SDK
0.16.8 newlib (`aarch64-zephyr-elf/include/math.h`) declares only `sqrtl` and
`hypotl`, and its `libm.a` defines no `fabsl`. picolibc implements binary128
itself and declares them all.

ASI works around it with a define keyed on the Zephyr board name
(`actuation_module/src/asi_common/CMakeLists.txt:51-56`):

    -Dfabsl(x)=((long double)fabs((double)(x)))

which also rounds the argument through double.

## Fix

`zephyr/cxx-compat/cmath`: the toolchain's `<cmath>` by `#include_next`, plus
`fabsl` and `copysignl` when the libc is newlib (not picolibc) and long double
is wider than double. Both are sign-bit operations; GCC expands
`__builtin_fabsl` / `__builtin_copysignl` inline for binary128, so no library
symbol is needed and the result is exact. Transcendentals (`expl`, `cosl`, ...)
have no exact form through double and stay undeclared, so they still fail at
compile time rather than losing precision silently. Each declaration is skipped
when the name is already a macro, so a consumer still carrying its own define
keeps building until it drops it. With no real `<cmath>` reachable (Zephyr's
minimal libcpp) the header errors, as the missing header did before.

`cxx-compat` is on every Zephyr C++ image's include path, so this reaches the
consumer's own libraries as well as `app`.

## Measured

Zephyr SDK 0.16.8 `aarch64-zephyr-elf-g++ -std=c++17 -Wpedantic -Werror`, a
TU including `<cmath>` and `<Eigen/Core>` (Eigen 3.4) calling `::fabsl`,
`copysignl`, `numext::abs` and `numext::absdiff` on long double:

- without the shim: the error above (and `'copysignl' was not declared`);
- with it, at `-O0` and `-O2`: compiles, undefined symbols only libgcc's
  `__addtf3` / `__subtf3` / `__fixtfsi`, no `fabsl`;
- picolibc (`--specs=picolibc.specs`): compiles, the shim inert.

## Consumer cleanup

ASI drops `actuation_module/src/asi_common/CMakeLists.txt:51-56` (the
`BOARD STREQUAL "fvp_baser_aemv8r"` block and its comment).
