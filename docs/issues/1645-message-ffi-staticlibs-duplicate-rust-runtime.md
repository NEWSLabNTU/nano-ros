---
id: 1645
title: "C++ Zephyr images whole-archive one message-FFI Rust staticlib per package, duplicating the Rust runtime closure — the last reason for --allow-multiple-definition"
status: open
type: tech-debt
area: build, zephyr, nuttx
severity: low
found: 2026-10-02
related: [1636, 1618, 0734, phase-251]
---

## What

Every C++ Zephyr image that uses message packages links one generated
`nano_ros_cpp_ffi_<pkg>` crate per package as its OWN cargo `staticlib`
(`libnano_ros_cpp_ffi_std_msgs.a`, `libnano_ros_cpp_ffi_builtin_interfaces.a`),
each WHOLE-ARCHIVED (issue 0056). A staticlib bundles its whole closure, so
the archives each carry `compiler_builtins` (and, on the host triple, a few
`core` functions and LLVM `anon.*` constants), and whole-archiving forces every
member in.

Measured by issue 1636, building `examples/zephyr/cpp/talker` (zenoh) with
`--allow-multiple-definition` removed:

| image | duplicate symbols | breakdown |
| --- | ---: | --- |
| native_sim/native/64 | 392 | 340 `compiler_builtins`, 35 `__*` intrinsics (`__absvdi2`, `__addvsi3`, `__cmpdi2`, ...), 15 `anon.*`, 2 `core` |
| mps2/an385 | 571 | all `compiler_builtins` |

Every duplicate is between the two FFI archives
(`libnano_ros_cpp_ffi_builtin_interfaces.a` against
`libnano_ros_cpp_ffi_std_msgs.a`), not against `libnros_cpp.a`. None is a
nano-ros C-ABI symbol, a `REGISTRY` or zenoh-pico state, so this is the benign
half of the multi-staticlib class. The flag lives in
`zephyr/cmake/nros_generate_interfaces.cmake`, beside the whole-archive line,
so only images that link an FFI archive get it.

`integrations/s32ds/makefile.defs` is the other allowlisted use. It is
UNVERIFIED: there is no S32DS project on the host. It links `corrosion/*.a`,
which can hold `libnros_c.a` beside `libnros_cpp.a`. On NuttX that pairing
measured 392 duplicates, including 297 `nros_*` and `REGISTRY` (issue 1636),
and was fixed by linking `libnros_cpp.a` alone. The same fix probably applies
here, but it has to be verified against a real S32DS link.

## What closing needs

- One Rust staticlib per image: the message FFI reaches the runtime crate as an
  RLIB (or the runtime and FFI are bundled by one generated staticlib crate).
  Then drop the `zephyr/cmake/nros_generate_interfaces.cmake` row.
- S32DS: emit only one runtime archive into `nros-libs.mk` (the NuttX fix) and
  confirm the CDT link without the flag on a real project. Then drop that row.

`check-no-allow-multiple-def` fails until each row is lowered.
