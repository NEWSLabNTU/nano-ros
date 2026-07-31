---
id: 366
title: "FreeRTOS cmake configure fails 'Detecting C compiler ABI info - failed': the ABI-detection try_compile links an executable against arm-none-eabi newlib, whose syscall stubs (_write/_sbrk/…) are unresolved"
status: resolved
resolved_in: "arm-freertos-armcm3.cmake CMAKE_TRY_COMPILE_TARGET_TYPE"
type: bug
severity: medium
area: freertos
related: [issue-0365, phase-321]
---

## Finding (2026-07-31, next freertos-chain blocker after #365)

With the #361/#356 codegen fix and the #365 board-include fix landed, `just
freertos build-fixtures` now fails earlier — at CMake CONFIGURE of the freertos
workspace:

```
-- Detecting C compiler ABI info - failed
-- Detecting CXX compiler ABI info - failed
CMake Error at packages/rmw/cffi/CMakeLists.txt:10 (add_subdirectory):
-- Configuring incomplete, errors occurred!
```

`CMakeError.log` shows the real fault — the ABI-detection link:

```
libc.a(libc_a-writer.o): in function `_write_r':
  writer.c:(.text._write_r+0x14): undefined reference to `_write'
libc.a(libc_a-closer.o):  undefined reference to `_close'
libc.a(libc_a-lseekr.o):  undefined reference to `_lseek'
libc.a(libc_a-readr.o):   undefined reference to `_read'
libc.a(libc_a-sbrkr.o):   undefined reference to `_sbrk'
collect2: error: ld returned 1 exit status
```

(The `add_subdirectory:10` error is downstream noise — configure aborts once ABI
detection reports the compiler "failed".)

## Root cause

`cmake/toolchain/arm-freertos-armcm3.cmake` sets `CMAKE_C_COMPILER_WORKS TRUE`
(which skips only the `CMakeTestCCompiler` "does it work" probe) but does NOT set
`CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY`. So CMake's SEPARATE
`CMakeDetermineCompilerABI` step still runs a `try_compile` that **links an
executable**. On the bare-metal `arm-none-eabi` toolchain, newlib's reentrant
wrappers pull in `_write`/`_read`/`_close`/`_lseek`/`_sbrk` syscall stubs that
nothing provides at ABI-detection time (the board's syscall stubs are linked only
in the real build, and there is no `--specs=nosys.specs`/`nano.specs` in the
detection flags). The link fails → "Detecting C compiler ABI info - failed" →
configure aborts before any target is built.

The standard bare-metal-CMake idiom for exactly this is
`set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)`, which makes `try_compile`
build a static library (compile-only, no link), so ABI detection succeeds without
a full-program link.

## Scope

None of the bare-metal toolchains set `CMAKE_TRY_COMPILE_TARGET_TYPE`
(`git grep TRY_COMPILE_TARGET_TYPE cmake/toolchain/`), but only the freertos one
trips this: `armv7a-nuttx`/`riscv32-nuttx` link NuttX's own libc (provides the
syscalls) and `riscv64-threadx` uses picolibc (`--specs=picolibc.specs`). So the
concrete fix is the freertos toolchain; adding the guard to the others is cheap
insurance against a libc change but not required today.

## Fix

`cmake/toolchain/arm-freertos-armcm3.cmake`: add
`set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)` (before the first
`project()`/enable_language, i.e. in the toolchain file). Then CMake ABI detection
compiles without linking and configure proceeds. (Alternative — add
`--specs=nosys.specs` to the detection link — is worse: it injects stub syscalls
that must not leak into the real image.)

## Impact

- `just freertos build-fixtures` cannot CONFIGURE the freertos workspace → the
  tier-2 `freertos,*` coordinates still cannot build. Third distinct blocker in the
  freertos chain (codegen #361 → include-path #365 → this cmake-ABI issue).
- Latent because full freertos fixture builds are rarely run from scratch locally.

## Repro

```
source ./activate.sh && source /opt/ros/humble/setup.bash
just freertos build-fixtures
# -- Detecting C compiler ABI info - failed  (CMakeError.log: undefined reference to _write/_sbrk/…)
```

## RESOLVED (2026-07-31)

`cmake/toolchain/arm-freertos-armcm3.cmake`: added
`set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)` next to the existing
`CMAKE_C{,XX}_COMPILER_WORKS TRUE` block, so CMake's ABI-detection `try_compile`
builds a static library (compile-only) instead of linking an executable — no
newlib syscall stubs needed at detection time.

**Verified:** `just freertos build-fixtures` no longer errors "Detecting C
compiler ABI info - failed" / `undefined reference to _write/_sbrk` — cmake
configure passes and the build proceeds to compilation.
