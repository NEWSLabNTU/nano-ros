---
id: 536
title: "Three west fixtures assert a configure-time fact but pay for a full kernel build, one of them for a link the script expects to fail"
status: open
type: performance
area: build, testing
related: [issue-0509, issue-0535, issue-0034, issue-0041]
---

## Problem

`scripts/build/west-fixtures.sh` builds four fixtures. Three of their consumers
never look at an image:

| fixture | consumer | what it actually asserts | needs an ELF? |
| --- | --- | --- | --- |
| `west_board_import` | `board_import.rs` | `CMakeCache.txt` (BOARD / `NANO_ROS_RMW` / `NROS_BOARD_RUNNER` propagation) — 4 references, nothing else | **no** |
| `zephyr_self_pkg_rust` | `zephyr_self_pkg.rs` | `nros-system/system_config.h` + `system_config.cmake` exist | **no** |
| `zephyr_self_pkg_sibling` | `zephyr_self_pkg.rs` | same | **no** |
| `west_bringup_zephyr` | `cli_bringup_zephyr.rs` | bake + **boots `zephyr.exe`** | yes |

The two self-pkg fixtures are the sharpest case. The script already knows the
link is pointless and says so at `west-fixtures.sh:112`:

> the contract is "the configure-time BAKE (`nros-system/system_{config.h,main.c}`)
> fires", NOT a full ELF link (the link needs the rest of the runtime — out of
> scope, same as the original test). So the stamp gate is BAKE-EXISTS, not
> west's exit code: `west build` configures (baking) then attempts the doomed
> link, and we stamp iff the bake landed.

So the build runs a link it expects to fail, discards the failure, and stamps on
a file produced before the link started. `west_board_import` is the same shape
without the commentary: a full `west build` on the FVP board so a test can read
four cache variables.

## Cost

Issue 0509 measured ~140 s of work per Zephyr leaf, dominated by fixed per-leaf
overhead (west + cmake startup, prep, signature, a cargo fingerprint pass) — a
configure-only fixture pays nearly all of that, because almost none of it is
compilation. On disk, `zephyr-workspace/` is **215 GB across 75 build dirs**
(measured 2026-08-13), mean ~2.8 GB per leaf.

## Direction

Demote the three to a configure-only builder. The manifest already has the
concept: `builder = "cmake-configure"` backs 12 `[[compile_check_fixture]]` rows
whose contract is exactly "configure succeeded, artifact X exists"
(`compile-check-fixtures.sh` → `build/cmake-fixtures/<id>`, asserted by
`require_cmake_fixture`). `west build --cmake-only` (or `-t` stopping before the
link) is the west-side equivalent.

Land it with issue 0535, not before: these fixtures need rows regardless, and
the builder field is where "configure only" gets DECLARED rather than implied by
a comment about a doomed link.

**Do not** simply delete the link and keep the same stamp logic. The stamp gate
would then be identical for a fixture that configures and one that configures
and links, which is how a build-only lane starts reading as covered — the
failure mode already recorded against the FVP aemv8r leaves in issue 0537.
