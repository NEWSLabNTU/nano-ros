---
id: 1636
title: "Four `--allow-multiple-definition` uses are load-bearing: Zephyr, NuttX and S32DS images still link more than one Rust staticlib"
status: resolved
resolved_in: 2026-10-03
type: tech-debt
area: build, zephyr, nuttx
severity: medium
found: 2026-10-02
related: [1618, 0734, phase-251, 0425, 1645, 1646]
---

## What

Issue 1618 widened `check-no-allow-multiple-def` to every build-file KIND and
found four live uses. Each is now an exact-count row in
`scripts/allow-multiple-def-allowlist.txt` owned by this issue:

| file | uses | what the flag hides |
| --- | ---: | --- |
| `zephyr/CMakeLists.txt` | 2 | (a) native_sim C/C++: picolibc's `malloc` against Zephyr's `COMMON_LIBC_MALLOC`, Zephyr's wins; (b) a non-native C++ image links `nros-cpp` AND `nros-rmw-zenoh-staticlib` |
| `integrations/nuttx/Make.defs` | 1 | `libnros_c.a` + `libnros_cpp.a` + per-package FFI staticlibs |
| `integrations/s32ds/makefile.defs` | 1 | the same multi-staticlib shape, in a user-editable S32DS fragment |

None can simply be deleted. (b) is the issue-0734 shape one layer down: a cargo
`staticlib` bundles its whole closure, so two of them define the same strong
symbols. Measured with `arm-nm -g --defined-only` on the
`build-cortex-m-cpp-talker-zenoh` archives (built 2026-09-10, so the census is
from that tree): `libnros_cpp.a` and `libnros_rmw_zenoh_staticlib.a` share
**1397** strong global symbols, among them `REGISTRY`,
`nros_rmw_cffi_register`, `nros_rmw_cffi_lookup` and the `nros_zephyr_heap_*`
family. So the backend registry exists twice and the flag picks which copy
every caller gets. That is the #48 wrong-copy hazard, and only the linker's
first-wins order keeps it consistent.

## What closing needs

A Zephyr / NuttX / S32DS image links ONE Rust staticlib, as the cmake path
already does (`cmake/NanoRosRuntimeCrate.cmake`'s single-runtime invariant):
the backend reaches the runtime crate as an RLIB feature, not as a second
staticlib. That is a design change to the backend selection on those three
paths (`nros-c` / `nros-cpp` deliberately dropped `dep:nros-rmw-zenoh` in
Phase 134.fix), so it is filed rather than done inside a gate fix. The
native_sim malloc case (a) is separate: it needs picolibc's allocator kept out
of the link, not a single staticlib.

Each removal lowers its row. The gate fails when a count falls and the row
does not, so the list can only shrink.

## Resolution (2026-10-03)

Each use was removed, the affected image was built, and the linker's duplicate
list was read. Zephyr images were built in an mx500 west workspace from this
tree: `examples/zephyr/{c,cpp,rust}/talker` (zenoh) on native_sim/native/64,
and `cpp/talker` on mps2/an385. The NuttX shell was built with
`just nuttx build-integration-app` (C API) and the same recipe with
`CONFIG_NROS_CPP_API=y` (C++ API). Four uses are now two.

**Zephyr native_sim (C and C++): the allocator use is GONE.** It hid two real
duplicate sets:

- **33 zenoh-pico system symbols** (`z_time_*`, `z_clock_*`, `_z_task_*`,
  `_z_mutex*`, `_z_condvar_*`). zpico's `platform_aliases.c` defined them in the
  Rust staticlib, and `zephyr/nros_zenoh_zephyr_system.c` defined them again
  with POSIX-shaped types. Only link order chose the Zephyr copy. Fixed at the
  source: a `NROS_PLATFORM_ALIASES_SKIP_SYSTEM` guard, defined by the zpico
  build script for Zephyr.
- **3 allocator symbols** (`malloc`, `free`, `aligned_alloc`). The host-std
  staticlib calls `posix_memalign`, which Zephyr's COMMON_LIBC_MALLOC lacks, so
  picolibc's allocator members were pulled in. Fixed: `heap_stub_native.c`
  defines `posix_memalign`/`memalign` on Zephyr's allocator.

The standalone **`nros-rmw-zenoh-staticlib` link is GONE** (C and C++
branches, plus the now-unused `_nros_zephyr_backend_features` macro).
`nros-c`/`nros-cpp` carry `dep:nros-rmw-zenoh` again (Phase 241.D3-rev), so it
was a second backend and a second `REGISTRY`. Each image now has exactly one
`REGISTRY`.

**Zephyr C++: scoped, not global.** `zephyr/CMakeLists.txt` carries no use any
more. Note that `examples/zephyr/c/talker` is a C++-API image too
(`NROS_API` is a Kconfig CHOICE and its `prj.conf` sets both, so CPP wins), so
an `if(CONFIG_NROS_CPP_API)` flag covered every C/C++ Zephyr image and masked
the two sets above all over again. The remaining use moved into
`zephyr/cmake/nros_generate_interfaces.cmake`, beside the line that
whole-archives a per-package C++ message-FFI staticlib, so it reaches only
images that link one. Measured: c/talker links with the flag ABSENT from
`build.ninja`; cpp/talker without it fails with 392 duplicates on native_sim
(340 `compiler_builtins`, 35 `__*` intrinsics, 15 `anon.*`, 2 `core`) and 571
on mps2/an385 (all `compiler_builtins`). Every one is between the two FFI
archives, `libnano_ros_cpp_ffi_builtin_interfaces.a` and
`libnano_ros_cpp_ffi_std_msgs.a` (both whole-archived), not against
`libnros_cpp.a`. No `nros_*`, no `REGISTRY`. That use stays, owned by issue
1645.

The native_sim C, C++ and Rust talkers each published to a local `rmw_zenohd`
(18 / 18 / 7 samples in 10 s). mps2 is link-verified only (not run under QEMU).

**NuttX `Make.defs`: the use is GONE.** The first pass at this issue called it
unverifiable ("no build recipe"). That was wrong: `just nuttx
build-integration-app` builds and links the kernel with the shell's
`EXTRA_LIBS`. It links no code that calls nano-ros, though, so a clean link
there proves nothing. The measurement re-ran the shell's own `ld` line with
`--allow-multiple-definition` removed, `-u` on every `nros_*` symbol (524), and
a stub for the app's `nros_app_register_backends` hook:

| archives on the line | duplicates |
| --- | ---: |
| `libnros_c.a` + `libnros_cpp.a` (old C++ shape) | **392** — 297 `nros_*`, the `rcl*`/`rmw_*` compat surface, QoS and `__NROS_SIZE_*` constants, and `REGISTRY` |
| `libnros_cpp.a` only (new C++ shape) | 0, links |
| `libnros_c.a` only (C shape), 299 `nros_*` forced | 0, links |

The 392 came from two DIFFERENT `nros-c` builds (crate hashes differ), each
with its own statics. So the flag was binding each caller to whichever copy ld
met first, and the registry was among them: the #48 hazard, live.
`libnros_cpp.a` defines all 299 of `libnros_c.a`'s `nros_*` symbols (only 18
crate-hash-mangled size probes differ), so a C++ image now links
`libnros_cpp.a` INSTEAD of `libnros_c.a`. Both configurations then build and
link through the real `make`, with the flag absent from the `ld` line. The
`extra_libs.mk` hook stays as a user hook. Nothing in-tree has written it
since Phase 212.M-F.12, and an FFI archive added there is now a link error.

**S32DS `makefile.defs`: UNVERIFIED, allowlisted under issue 1645.** S32DS
3.6.10 is installed on this host (`~/NXP/S32DS.3.6.10`), but there is no S32DS
PROJECT. The shell's configure needs one (`NXP_S32DS_PROJECT` with a
`.cproject`, the RTD drivers, FreeRTOS and lwIP), and the final link is the CDT
project's. Its `nros-libs.mk` links `corrosion/*.a`, which can hold
`libnros_c.a` and `libnros_cpp.a` together. That is the shape NuttX measured at
392 duplicates, so it is the likely state, though it was not measured here.

`check-no-allow-multiple-def` was proven by mutation. Re-adding the flag to
`integrations/nuttx/Make.defs` or to `zephyr/CMakeLists.txt` fails it (rc 1),
naming the line.

**Found on the way, filed as issue 1646:** under `just` (`RUSTC_WRAPPER=sccache`),
a relinked `libnros_cpp.a` kept the OLD build-script object after
`platform_aliases.c` changed, while cargo reported the graph built. It cost
three misleading rebuilds before the cause was clear.
