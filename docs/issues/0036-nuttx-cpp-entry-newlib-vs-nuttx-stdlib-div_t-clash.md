---
id: 36
title: NuttX cpp entry — newlib vs NuttX stdlib.h `div_t` conflicting declaration
status: open
type: bug
area: c-api
related: [issue-0027, issue-0034, phase-235]
---

> **FIX LANDED 2026-06-12 (pending e2e confirmation).** `nuttx_ffi_build.rs`
> (`run_nuttx`) now adds `${NUTTX_DIR}/include/cxx` to the C++ compile's include
> search **ahead of** the cmake-passed `${NUTTX_DIR}/include`, so `<cstdlib>`
> resolves to NuttX's own wrapper (which pulls NuttX's `<stdlib.h>`) instead of
> libstdc++'s — the libstdc++ `#include_next <stdlib.h>` that reached newlib never
> fires. `<type_traits>` (not under `include/cxx/`, required by `node.hpp`) still
> falls through to libstdc++, so this is lighter than NuttX's own `-nostdinc++`.
> Verified in isolation with the real arm-none-eabi-g++ + NuttX headers: a TU
> including `<type_traits>` + `<cstdlib>` + `<stdlib.h>` failed with the exact
> `div_t` clash and compiled clean once `include/cxx` was prepended. Awaiting an
> e2e dispatch on the nuttx cell to confirm the full talker fixture.

The NuttX **C++** entry compile pulls two libc header sets and they clash on
`div_t`. Surfaced by the honest platform-ci e2e run 27393704883 (nuttx cell,
Test/e2e step) building the cpp talker fixture:

```
arm-none-eabi-g++ ... -c .../qemu-arm-nuttx/cpp/talker/build-zenoh/nros-entry/main.cpp
/github/home/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/arm-none-eabi/include/stdlib.h:39:3:
  error: conflicting declaration 'typedef struct div_t div_t'
.../stdlib.h:45:3: error: conflicting declaration 'typedef struct ldiv_t ldiv_t'
.../stdlib.h:52:3: error: conflicting declaration 'typedef struct lldiv_t lldiv_t'
error occurred in cc-rs: command did not execute successfully (status code exit status: 1)
error: failed to run custom build command for `nros-nuttx-ffi v0.4.0`
```

## Root cause

The cpp entry's `nros-nuttx-ffi` build.rs invokes cc-rs with
`-I third-party/nuttx/nuttx/include` **and** uses `arm-none-eabi-g++`, which
implicitly adds its own newlib `arm-none-eabi/include`. Both ship a `stdlib.h`
declaring `div_t`/`ldiv_t`/`lldiv_t` with incompatible struct shapes (NuttX's
named-struct form vs newlib's anonymous-typedef form) → C++ rejects the
redeclaration (C tolerated it / it never compiled this far before).

This is the **C++-path analogue of issue-0027 #1**, which fixed only the *C*
message-lib path: `812234321` added `${NUTTX_DIR}/include` as a **SYSTEM** include
to the NuttX NanoRos cmake umbrella so NuttX's headers win over bare newlib's
`__rtems__`-gated decls. The cpp FFI cc-rs compile got no such precedence — its
`-I` is a plain include, so newlib's `stdlib.h` is still reachable and collides.

## Why only now

The nuttx e2e/fixture path rarely ran to completion (cancelled on push churn,
ENOSPC, or earlier compile death). Phase-240's disk + nros-node compile fixes let
the nuttx cell run ~10 min into the cpp `build-fixtures` matrix for the first
time, exposing it. Same "lane now runs honestly" theme as [issue 0034] (where it
is also noted as a sibling-lane finding).

## Fix direction

Give the NuttX sysroot include **precedence** in the cpp FFI compile (the
cc-rs analogue of 0027 #1): add `${NUTTX_DIR}/include` as a SYSTEM/`-isystem`
include ahead of the toolchain default, or suppress the toolchain's newlib libc
headers for the NuttX entry (`-nostdinc` + explicit NuttX include set, matching
how the C path resolves). Verify both the arm (cortex-a7) and riscv NuttX cpp
talkers compile clean afterward.

Owner: nros-cpp / NuttX C++ header integration (issue-0027 follow-up).
