---
id: 1618
title: "`--allow-multiple-definition` is live in three build files that `check-no-allow-multiple-def` never reads, while its allowlist says the flag is gone"
status: open
type: bug
area: build, zephyr, nuttx
severity: medium
found: 2026-10-01
related: [phase-251, phase-472, 1614, 0734]
---

## What

`scripts/allow-multiple-def-allowlist.txt` is empty and says why:

> phase-251 W1 + W2 removed both uses … The invariant is now fully enforced:
> ANY `--allow-multiple-definition` fails the gate.

It is not enforced. The gate reads `cmake/**`, `scripts/**`, `just/**`, the
examples/packages CMake files and the root `CMakeLists.txt`/`justfile`. These
three live uses sit outside that population:

| file | use | why it is there |
| --- | --- | --- |
| `zephyr/CMakeLists.txt:225` | `zephyr_ld_options(-Wl,--allow-multiple-definition)` | native_sim: lets Zephyr's malloc win over picolibc's |
| `zephyr/CMakeLists.txt:236` | `zephyr_ld_options(-Wl,--allow-multiple-definition)` | C++ Zephyr images that link several Rust staticlibs: first lang-item definition wins |
| `integrations/nuttx/Make.defs:70` | `EXTRA_LIBPATHS += --allow-multiple-definition` | NuttX: `libnros_c.a` + `libnros_cpp.a` and overlapping FFI staticlibs |

The flag hides the #48-class wrong-copy hazard: whichever definition the linker
meets first wins. It also hides issue 0734's duplicated-closure shape. The second
Zephyr use describes exactly the multi-staticlib link that issue 0734 says must
not exist.

## How it was found

The 2026-10-01 gate re-run
([findings](../development/audit-findings-2026-10-01-rerun.md), new audits). The
same `-Wl,--allow-multiple-definition` appended to
`zephyr/cmake/nros_cargo_build.cmake` passes the gate (rc 0). Appended to
`cmake/NanoRosLink.cmake`, it fails (rc 1). Moving the population onto
`scripts/lib/file_kinds.py` (`cmake shell just make ci`) finds these three live
uses at once, so the gate cannot be widened without turning `main` red.

## What closing needs

For each use, a ruling: either remove it (the Zephyr C++ case should be a
single-runtime-staticlib link per issue 0734), or allowlist it with a reason
and an owning issue. Then widen the gate's population to the kinds, together
with the 1614 W5 sweep.
