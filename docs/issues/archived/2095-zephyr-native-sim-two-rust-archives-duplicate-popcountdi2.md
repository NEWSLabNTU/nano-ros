---
id: 2095
title: "Every Zephyr native_sim C++ image fails to link: two Rust staticlibs each carry compiler-builtins' `__popcountdi2`, and the second is whole-archived"
status: resolved
type: bug
area: [zephyr, build, cmake, link]
severity: high
found: 2026-10-27
related: [phase-466, 2032, 1645, 1664]
resolved: 2026-10-29
resolved_in: "duplicate of 2092 (44eab11c22)"
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-27. It stops the
Zephyr family of `run-matrix` and the tier-2 nightly. It was hidden behind
[issue 2032](2032-zephyr-kernel-arm-lacks-lock-storage-probes.md) until that
issue's second route was fixed.

## Measured

On origin/main `e6cafaf552` and later, x86_64 host, Zephyr 3.7,
`native_sim/native/64`, through the CI road
(`NROS_ZEPHYR_FIXTURE_FILTER='^build-cpp-talker-<rmw>$' just zephyr build-fixtures`):

| leaf | result |
| --- | --- |
| `build-cpp-talker-zenoh` | link FAILS: `multiple definition of '__popcountdi2'` |
| `build-cpp-talker-xrce` (with issue 2032's second route fixed) | link FAILS: the same, and nothing else |

The zenoh leaf fails the same way with that fix stashed, so the fix does not
cause it. The scheduled runs at `9f2d7b4bb3` (2026-10-26) do not report it:
there the zenoh C++ leaves linked, and the XRCE ones failed on issue 2032's
platform collisions alone. So it arrived between `9f2d7b4bb3` and
`e6cafaf552`. That range has not been bisected.

## Cause, as far as it is measured

`ld --trace-symbol=__popcountdi2` on the failing link:

```
libzephyr.a(bitarray.c.obj): reference to __popcountdi2
libnros_cpp.a(45c91108d938afe8-popcountdi2.o): definition of __popcountdi2
libnano_ros_cpp_ffi_image.a(45c91108d938afe8-popcountdi2.o): multiple definition of `__popcountdi2'
```

- Zephyr compiles `lib/utils/bitarray.c` unconditionally, and its popcount
  is a libgcc call.
- Two Rust staticlibs are on the line: `libnros_cpp.a` (Corrosion) and the
  image's message-FFI staticlib `libnano_ros_cpp_ffi_image.a` (issue 1645).
  Each embeds compiler-builtins, whose `__popcountdi2` is a STRONG `T`
  symbol in both (`nm`).
- `libnros_cpp.a` comes first, so it satisfies bitarray's reference. The FFI
  staticlib is then linked `--whole-archive`, which brings in its copy too.

That only explains the collision once the link order is what it is now. What
moved inside the range above is not established. Candidates are the cmake
changes in `192f1e5c5d` (phase-518 W2/W4/W5, Zephyr's
`nros_platform_link_app` arm) and `b9512210ac`.

## Bisected (2026-10-27): `8608f29c95`

`git bisect run` between `bed22e9d92` (good) and `f69c606cd6` (bad), one
clean `build-cpp-talker-zenoh` per step, named `8608f29c95` ("phase-518
W4/W5: build a stock ament workspace for FreeRTOS and Zephyr"). The
`192f1e5c5d` guess above was wrong: `192f1e5c5d^` is already bad.

That commit routed every app link through `nros_link_app_dependency`. The
helper appends to `LINK_LIBRARIES`, so a CMakeLists that uses the plain
`target_link_libraries` signature stays legal (ros2/demos `composition`). For
an executable that is all `target_link_libraries(<t> PRIVATE <lib>)` writes.
For a STATIC library it is half: PRIVATE also writes `$<LINK_ONLY:<lib>>`
into `INTERFACE_LINK_LIBRARIES`, and that is the only way a static library's
dependency reaches a final link. Zephyr's `app` is a static library. The
runtime therefore stopped being one of `app`'s own link items, and CMake
emitted it right after the component library that depends on it:

| link line around the FFI | |
| --- | --- |
| 10-23 (good) | `libtalker_lib.a`, `--whole-archive libnano_ros_cpp_ffi_image.a`, `libnros_cpp.a` |
| `8608f29c95` and later | `libtalker_lib.a`, `libnros.a`, `libnros_cpp.a`, `--whole-archive libnano_ros_cpp_ffi_image.a` |

With the runtime ahead of the FFI, `libnros_cpp.a` satisfies bitarray's
`__popcountdi2`, and the whole-archive then adds the FFI staticlib's copy.

## Fix

`nros_link_app_dependency` also appends `$<LINK_ONLY:<lib>>` to
`INTERFACE_LINK_LIBRARIES` when the target is a STATIC library, which is
the half PRIVATE writes. The user's signature stays free.
`cmake-ament-surface` case 13 holds both halves, plus an executable as the
control, and fails with the fix reverted.

Measured on origin/main `509b31606c` plus the fix, through the CI road, every
one of these links:
- `build-cpp-{talker,listener,service-server,action-client}-{zenoh,xrce,cyclonedds}`;
- `build-c-talker-xrce`;
- the four ported `build-cortex-m-cpp-port-*-zenoh` leaves, which are the
  phase-518 roads the helper was written for.

## What would close it

A tier-2 nightly and a scheduled `run-matrix` in which the Zephyr
`native_sim` C++ leaves link. Do not bring back `--allow-multiple-definition`:
issues 1645 and 1664 removed it from the tree. The likelier shapes are one
Rust runtime archive per image, or an order in which the whole-archived FFI
staticlib is the one that defines compiler-builtins.

## Closed (2026-10-29): duplicate of issue 2092

Issue 2092 was filed for the same defect (an independent sighting) and
fixed it in `44eab11c22`. `nros_second_staticlib_weaken_command()` weakens
every global of the per-image message-FFI staticlib except its own
`nros_cpp_*` surface, so `libnros_cpp.a` is the one definition of
compiler-builtins' intrinsics.

Verified on main `e0f81b4ac`:
- Locally, all six native_sim C++ zenoh leaves and all six C++ xrce leaves
  link with no `multiple definition`, and their nine e2e cells pass.
- In CI, tier-2 nightly 40648207713 built and PASSED the three Zephyr C++
  xrce cells. That image class failed on this exact symbol after issue
  2032's fix. The nightly's lane did not select the zenoh C++ coordinates,
  so they have no CI result here; the local run covers them.
