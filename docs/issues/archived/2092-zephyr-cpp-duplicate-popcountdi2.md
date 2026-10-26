---
id: 2092
title: "Every Zephyr native_sim C++ image fails to link: `multiple definition of __popcountdi2` between the whole-archived message-FFI staticlib and libnros_cpp.a"
status: resolved
type: bug
area: [zephyr, build, cpp]
severity: high
found: 2026-10-27
resolved: 2026-10-27
related: [1645, 1664, 0056, 1467, 2032, phase-517]
---

## Measured

Seen during phase-517 W5 evidence on the Zephyr native_sim lane, and present on
`origin/main` (`bd677bb369`). Reproduced in a clean worktree with
`NROS_ZEPHYR_FIXTURE_FILTER=build-cpp-talker-zenoh just zephyr build-fixtures`:

```
ld.bfd: …/build-cpp-talker-zenoh/nano_ros_cpp_ffi_image/target/x86_64-unknown-linux-gnu/nros-minsizerel/libnano_ros_cpp_ffi_image.a(45c91108d938afe8-popcountdi2.o):
…/library/compiler-builtins/compiler-builtins/./lib/builtins/popcountdi2.c:19: multiple definition of `__popcountdi2';
nros-rust/x86_64-unknown-linux-gnu/nros-relwithdebinfo/libnros_cpp.a(45c91108d938afe8-popcountdi2.o): first defined here
```

With the fix mutated out and every FFI archive forced to rebuild, the Zephyr
C++ leaves split as follows:

| leaves | result |
| --- | --- |
| `build-cpp-{talker,listener,service-server,service-client,action-server,action-client}-zenoh` (native_sim/native/64) | 6 / 6 FAIL, `__popcountdi2` |
| the same six, `-cyclonedds` | 6 / 6 FAIL, `__popcountdi2` |
| `build-cortex-m-cpp-*-zenoh` (mps2/an385, 5 leaves) | 5 / 5 link |

## Cause

Two Rust staticlibs reach one C++ Zephyr image, and both carry
`compiler_builtins` built with its `c` feature on the host triple: 34
compiler-rt intrinsics compiled from C (`__popcountdi2`, `__cmpdi2`,
`__muldc3`, `__absvdi2`, …), each a separate archive member defining a STRONG
global. Fat LTO internalizes every Rust symbol in the FFI archive (phase-336's
`cpp-ffi-glue` carve-out) and cannot reach these, because they are not Rust.
Measured on the talker: the FFI archive's global definitions are exactly the
96 `nros_cpp_*` symbols plus those 34 intrinsics and the weak
`__compilerrt_abort_impl`.

1. `libnros_cpp.a` (the runtime) is linked normally, so ld pulls a member only
   to satisfy a pending reference.
2. `libnano_ros_cpp_ffi_image.a` (issue 1645's one-per-image message FFI) is
   WHOLE-ARCHIVED (issue 0056), after the runtime, so all 34 intrinsics are
   forced in.
3. `zephyr/libzephyr.a` is whole-archived ahead of both and contains
   `lib/utils/bitarray.c`, which calls `__popcountdi2` (GCC lowers
   `__builtin_popcountll` to it without `-mpopcnt`). The runtime is the first
   archive that defines it, so ld pulls `popcountdi2.o` from `libnros_cpp.a`,
   and the whole-archive then defines it a second time.

Which intrinsic collides depends only on which ones the image's C happens to
call, so the defect is the 34, not `__popcountdi2`. The mps2/an385 leaves link
because their Rust is built with `-Z build-std=core,alloc,compiler_builtins`,
which ships no C intrinsics.

Issue 1645 noted these 35 `__*` symbols among the duplicates between two FFI
archives, and its fix measured the talker linking clean with one FFI archive.
What changed since then to let a runtime member be pulled ahead of the
whole-archive was not bisected.

## Resolution

`cmake/NanoRosSecondStaticlib.cmake` adds
`nros_second_staticlib_weaken_command()`. It returns an `objcopy --wildcard
--weaken-symbol=!<keep> --weaken-symbol=*` command over an archive. The rule it
encodes: a second Rust staticlib in one image keeps its own surface strong and
weakens every other global it defines. A weak definition never conflicts. When
ld has pulled the runtime's strong copy, that copy wins, and when nothing else
defines the name the weak one serves. Both copies are the same compiler-rt
source from the same toolchain. The surface stays strong so that a real
double definition of an FFI symbol is still a link error.

`_nros_zephyr_cpp_ffi_image()` runs that command, with `KEEP "nros_cpp_*"`,
as a second `COMMAND` of the custom command that produces the archive. It is
idempotent, so a rebuild after a fresh cargo is harmless. Both archives still
link, in the same order. Issue 1645's end state, one Rust staticlib per image
(the runtime and the FFI glue bundled in one generated crate), would remove the
second archive altogether, but that changes which runtime archive a C++ image
links, so it is not done here.

**The class.** A sweep of every `.cmake` / `CMakeLists.txt` function that both
resolves cargo (`nros_rust_tool(<v> cargo)`) and names `--whole-archive` finds
exactly one: `_nros_zephyr_cpp_ffi_image`. The other whole-archive sites link C
archives: the RMW backend flag in the root `CMakeLists.txt`, the Cyclone type
support, and `nano_ros_support_library()`. The canonical road's per-package FFI
archives are linked lazily, so a member there is pulled at most once.

**Gate:** `just check cmake-second-staticlib` (fast line,
`tests/cmake-second-staticlib-tests.sh`):

- A: synthetic runtime and FFI archives shaped like the real ones. The
  unweakened pair must fail with `multiple definition of __popcountdi2`, which
  is the negative control. After the helper runs, the intrinsic is `W`,
  `nros_cpp_*` is still `T`, and the pair links.
- B: idempotence.
- C: the class scan above. Every member must compute the helper's command and
  run it as a `COMMAND`. The Zephyr FFI function must be found as a member.

Mutation-checked. With the `COMMAND ${_ffi_weaken}` line commented out, C fails
(`UNRUN zephyr/cmake/nros_generate_interfaces.cmake:656
_nros_zephyr_cpp_ffi_image`).

**Sweep, with every FFI archive forced to rebuild:**

| leaves | before (fix mutated out) | after |
| --- | --- | --- |
| native_sim C++ zenoh (6) | 6 FAIL | 6 link |
| native_sim C++ cyclonedds (6) | 6 FAIL | 6 link |
| mps2/an385 C++ zenoh (5) | 5 link | 5 link |
| native_sim C++ xrce (6) | — | 6 FAIL, **issue 2032** (the POSIX platform port inside `libnros_cpp.a`, `nros_platform_clock_ns` etc.), unrelated to this fix |

Runtime: `build-cpp-listener-zenoh` and `build-cpp-talker-zenoh` were run
against `rmw_zenohd` on `tcp/127.0.0.1:7600`. The listener printed
`I heard: [Hello World: 1]` and onwards, 22 messages in about 10 s.
