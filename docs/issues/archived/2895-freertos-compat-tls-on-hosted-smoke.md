---
id: 2895
title: "The FreeRTOS C-port smoke no longer links on the host: `cyclonedds_compat.c` demands the ARM-only `__tls_base`"
status: resolved
type: bug
area: [platform, freertos]
severity: high
found: 2026-11-23
resolved: 2026-11-23
related: [phase-582, phase-590]
---

## What fails

Nightly 45115997343 (main `a1cbbb6b04`), FreertosPosix "Platform C-port
smoke" (`just freertos test-c-port`):

```text
cyclonedds_compat.c:(.text+0xb): undefined reference to `__tls_base'
```

## Cause

`cyclonedds_compat.c` fills in what a bare-metal image lacks:
`__aeabi_read_tp` (ARM EABI TLS, reading the linker script's `__tls_base`),
`gethostname` and `clock_gettime`. Its CMake option defaulted ON, and each
hosted build had to turn it off by hand. The freertos-posix board did; the
C-port smoke (`tests/freertos-c-smoke`) did not. That was harmless while no
code called anything the TU defines, because the archive member was never
pulled. 8df9ecad4d (phase-582 W1d) made `platform.c` call `clock_gettime`,
so on the host the linker took this TU's `clock_gettime` over glibc's, and
with it `__aeabi_read_tp` and its missing `__tls_base`.

## Resolution

- `NROS_PLATFORM_FREERTOS_WITH_BAREMETAL_COMPAT` now defaults from the
  target: ON only when `CMAKE_SYSTEM_NAME` is `Generic` (every in-tree
  FreeRTOS cross toolchain sets it), OFF on a hosted system. An explicit
  setting still wins.
- `__aeabi_read_tp` is compiled only under `__ARM_EABI__`, the only target
  that calls it.

Reproduced on the smoke's own configure and link: undefined `__tls_base`
without the fix, a clean link with it.

Sweep: `git grep -n 'WITH_BAREMETAL_COMPAT'` (two hand-set OFFs remain and
are now redundant) and `git grep -n -E '__aeabi_[a-z_]+\s*\(' -- '*.c'` (this
is the only ARM EABI helper defined in our sources).
