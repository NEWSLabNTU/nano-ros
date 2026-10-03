---
id: 1645
title: "C++ Zephyr images whole-archive one message-FFI Rust staticlib per package, duplicating the Rust runtime closure — the last reason for --allow-multiple-definition"
status: resolved
resolved_in: 2026-10-03
type: tech-debt
area: build, zephyr, nuttx
severity: low
found: 2026-10-02
related: [1636, 1618, 0734, phase-251, 1664]
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

## Resolution (2026-10-03)

**Zephyr: fixed.** `zephyr/cmake/nros_generate_interfaces.cmake` builds ONE
message-FFI staticlib per image (`libnano_ros_cpp_ffi_image.a`) instead of one
per package.

- Each `nros_generate_interfaces(... CPP)` call now only contributes its
  closure (dependency TYPES plus its own EXPORTS, phase-306 W1's split) to a
  global property.
- The first call defers `_nros_zephyr_cpp_ffi_image()` to the end of the app
  directory (`cmake_language(DEFER)`). That function writes a single crate
  over the de-duplicated union, builds it and whole-archives it once.
- An exports file belongs to exactly one package, so no `nros_cpp_*` symbol
  can appear twice. `compiler_builtins` now reaches the link once.
- The flag and its allowlist row are gone.

Measured on `examples/zephyr/cpp/talker` (zenoh, native_sim/native/64):

| layout | flag | link |
| --- | --- | --- |
| per-package archives (`main`) | removed | **392** `multiple definition` errors, the issue's count |
| one image archive (this change) | removed | links clean |

With the fix, `build.ninja` names only `libnano_ros_cpp_ffi_image.a` and no
`--allow-multiple-definition`. The image was run against an `rmw_zenohd`
router started and stopped by hand. A stock `ros2 topic echo /chatter`
(`rmw_zenoh_cpp`) received `data: 'Hello World: 3'`. The two `.eh_frame_hdr`
linker notes also appear in `main`'s build, so they are not new.

NOT measured: mps2/an385. Its 571 duplicates were the same FFI-vs-FFI class,
and the change is board-independent, but only native_sim was built here.

**S32DS: not feasible on this host**, and moved to **issue 1664**. S32DS
3.6.10 is installed, but no S32DS project exists, and the CDT link that the
flag affects is the project's. The row stays allowlisted under 1664.

