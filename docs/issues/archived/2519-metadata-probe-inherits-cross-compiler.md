---
id: 2519
title: "The host metadata probe inherits a cross configure's compiler, and its failure is cached so images silently keep default sizes"
status: resolved
type: bug
area: [tooling, build, sizing]
severity: high
found: 2026-11-14
related: [issue-1593, issue-2097, phase-550]
---

**Home: the C/C++ metadata probe (`orchestration/metadata_probe_cmake.rs`,
`orchestration/metadata_refresh.rs`)**

## Symptom

On main, the tier-2 nightly (runs 43172962836 and 43383470982, 2026-11-12 and
2026-11-13) printed, for two standalone C leaves:

```
sync: source metadata — no producer for riscv64_threadx_c_typed_carrier::c_typed_carrier
  (probe failed at this source last sync; unchanged; last error: probe project
  configure failed: metadata probe configure failed for `workspace` (exit 1):)
nros build:   resolved → … (entity counts not derived; configured defaults kept)
```

and the same for `nuttx_c_typed_carrier`. Those images were built on DEFAULT
pool sizes. The build did not fail, so nothing turned red. Locally, a NuttX
fixture build left five workspaces' probe caches (`workspaces/c`, `cpp`,
`realtime-c`, `realtime-cpp`, `qemu-armv7a-nuttx/cpp/talker`) recording
`CMAKE_C_COMPILER=arm-none-eabi-gcc` under a `Linux` CMakeSystem. Every probe
there failed CMake's compiler check (undefined `_exit`, `_write`, …), and the
make road's arm project case then refused with "configured without an entity
inventory".

## Root cause

Three defects compound.

1. **CMake exports the compiler it configured to every child of a configure.**
   Measured on CMake 3.22, a `project(p C CXX ASM)` that runs `env` through
   `execute_process` sees `CC`, `CXX` and `ASM`. They are `/usr/bin/cc` on a
   host configure and the cross compilers under a cross toolchain file. This is
   CMake's behaviour, not any platform's. Every cmake road reaches `nros
   configure` through `cmake/NanoRosConfigureHook.cmake`, and `nros configure`
   runs the HOST metadata probe. So on every cross road (NuttX, FreeRTOS,
   ThreadX, Zephyr, bare metal), whenever the probe's first configure happens
   under a cross configure, the probe caches the cross compiler for good.
   CMake never swaps a cached compiler in place.
2. **A whole-project configure failure was negative-cached.** The marker is
   keyed on the components' sources, the CLI stamp and the probe closure, none
   of which an inherited compiler, a missing tool or a broken cache changes.
   So the marker outlived exactly the repairs that could fix it, and each later
   sync printed one quiet line.
3. **The recorded reason dropped CMake's error.** The reason picker looked for
   `error:`, `error[` or a leading `error`. CMake's `CMake Error at <file>:<n>
   (<cmd>):` matches none of them, so the picker fell back to the first line,
   the `failed for `workspace` (exit 1):` header. That is why CI's "last error"
   is empty.

## Fix

- `pin_probe_env` removes every compiler variable CMake exports (one per
  language), the flag variables a first configure seeds from, and
  `CMAKE_TOOLCHAIN_FILE`. The probe is a host build whoever starts it.
- Before configuring, a probe cache whose `CMAKE_C_COMPILER` does not build for
  this host (asked with `-dumpmachine`) has its `CMakeCache.txt` and
  `CMakeFiles` RENAMED to `*.cross-<ts>`. They are kept as evidence, never
  deleted, a warning is printed, and the probe configures afresh. This repairs
  caches that are already poisoned.
- A whole-project configure failure writes no marker. It prints a WARNING that
  the batch keeps default sizes this sync, with CMake's error. As with issues
  1593 and 2097, the next sync retries.
- `first_error_line` recognises `CMake Error`.

A failure in one component's own probe build still gets a marker: that failure
is about the component's inputs.

## Verification

- Unit tests: `the_probe_inherits_no_compiler_from_its_caller`,
  `a_cache_recording_a_cross_compiler_is_set_aside` (a fake compiler answering
  `arm-none-eabi`), `a_host_or_unaskable_cache_is_kept`, and
  `a_cmake_configure_error_is_the_recorded_reason`.
