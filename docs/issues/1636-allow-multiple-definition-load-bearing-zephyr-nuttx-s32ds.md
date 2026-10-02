---
id: 1636
title: "Four `--allow-multiple-definition` uses are load-bearing: Zephyr, NuttX and S32DS images still link more than one Rust staticlib"
status: open
type: tech-debt
area: build, zephyr, nuttx
severity: medium
found: 2026-10-02
related: [1618, 0734, phase-251, 0425]
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
