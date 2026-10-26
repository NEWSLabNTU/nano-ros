---
id: 2095
title: "Every Zephyr native_sim C++ image fails to link: two Rust staticlibs each carry compiler-builtins' `__popcountdi2`, and the second is whole-archived"
status: open
type: bug
area: [zephyr, build, cmake, link]
severity: high
found: 2026-10-27
related: [phase-466, 2032, 1645, 1664]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-27. It stops the
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

## What would close it

A tier-2 nightly and a scheduled `run-matrix` in which the Zephyr
`native_sim` C++ leaves link. Do not bring back `--allow-multiple-definition`:
issues 1645 and 1664 removed it from the tree. The likelier shapes are one
Rust runtime archive per image, or an order in which the whole-archived FFI
staticlib is the one that defines compiler-builtins.
