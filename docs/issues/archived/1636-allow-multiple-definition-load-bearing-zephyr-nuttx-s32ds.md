---
id: 1636
title: "Four `--allow-multiple-definition` uses are load-bearing: Zephyr, NuttX and S32DS images still link more than one Rust staticlib"
status: resolved
resolved_in: 2026-10-02
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

## Resolution (2026-10-02)

Each use was removed, the affected image was built, and the linker's duplicate
list was read. Images were built in an mx500 west workspace from this tree:
`examples/zephyr/c/talker` and `cpp/talker` (zenoh) on native_sim/native/64,
and `cpp/talker` on mps2/an385.

**Zephyr native_sim (C and C++): the use is GONE.** It was hiding two real
duplicate sets, not one:

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
was a second backend and a second `REGISTRY`. After the change, each image has
exactly one `REGISTRY` (`readelf`). The native_sim C and C++ talkers each
publish 15 samples to a local `zenohd` in 8 s. mps2 is link-verified only (not
run under QEMU).

**What stays (3 rows), owned by issue 1645:**

- `zephyr/CMakeLists.txt` (C++ API): `libnano_ros_cpp_ffi_std_msgs.a` beside
  `libnros_cpp.a`. 392 duplicates (native_sim) / 571 (mps2), every one of them
  `compiler_builtins` / `core` / `alloc` / `anon.*`, with no nano-ros C-ABI
  symbol.
- `integrations/nuttx/Make.defs`, `integrations/s32ds/makefile.defs`:
  **UNVERIFIED**. There is no build recipe for the NuttX apps-external shell,
  and no S32DS, on this host. Both link the same FFI shape, and the NuttX
  shell also links `libnros_c.a` beside `libnros_cpp.a`.

**Found on the way, filed as issue 1646:** under `just` (`RUSTC_WRAPPER=sccache`),
a relinked `libnros_cpp.a` kept the OLD build-script object after
`platform_aliases.c` changed, while cargo reported the graph built. It cost
three misleading rebuilds before the cause was clear.
