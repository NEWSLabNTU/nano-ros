---
id: 530
title: "`FREERTOS_PORT` is upstream's variable name with incompatible values —
  ours is a path fragment, upstream's an enum"
status: resolved
type: bug
area: build-system
related: [rfc-0072, phase-349]
---

## The collision

Upstream `FreeRTOS-Kernel` takes `FREERTOS_PORT` as an **enum**, keyed into a
1356-line generator-expression table in `portable/CMakeLists.txt`:

```cmake
set(FREERTOS_PORT GCC_ARM_CM4F CACHE STRING "")
```

nano-ros takes the same variable name as a **path fragment** under `portable/`:

```cmake
# packages/api/nros-c/cmake/nros-freertos.cmake
set(_port_dir "${FREERTOS_DIR}/portable/${_NFBK_PORT}")   # GCC/ARM_CM3
```

Same name, incompatible values, no validation. A user arriving from upstream's
own documentation, any FreeRTOS tutorial, or a FreeRTOS-Plus-TCP demo sets
`GCC_ARM_CM4F` and nano-ros silently looks for `portable/GCC_ARM_CM4F/port.c`.

## Why it was a bad failure

The old diagnostic was whatever cmake says about a missing source file — a
`.c` path the user never typed, reported from inside `add_library`, with no
mention of `FREERTOS_PORT` and no hint that two spellings exist. Reusing a
well-known variable name for a different value space is the worst version of
this: it accepts a plausible value and fails somewhere unrelated.

## Fix

`nros_freertos_build_kernel()` now accepts **either** spelling. Upstream's enum
is translated (`GCC_ARM_CM4F` → `GCC/ARM_CM4F` — the compiler is the first
underscore-separated token in every upstream port name) and the translation is
announced with a `message(STATUS)` rather than done silently.

If neither spelling resolves, it fails **at the variable**, naming both
accepted forms and listing the compiler directories actually present under
`${FREERTOS_DIR}/portable`.

Verified against a stub kernel tree with all three cases: our path fragment
resolves; upstream's enum resolves with the translation notice; a port that
does not exist fails with the guidance rather than a missing-source error.

## Note on direction

Accepting upstream's spelling is deliberately the forward-compatible choice:
[phase-349](../roadmap/phase-349-rtos-integration-shells.md) W3 retires this
builder in favour of upstream's own `CMakeLists.txt`, at which point upstream's
enum becomes the only vocabulary and this translation layer is deleted along
with the function. Translating toward upstream now means the eventual migration
changes nothing for anyone who has already written the enum.
