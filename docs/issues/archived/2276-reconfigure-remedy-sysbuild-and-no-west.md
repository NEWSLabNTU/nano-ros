---
id: 2276
title: "The re-configure remedy assumes a non-sysbuild west build: it names the image subdir under sysbuild, and `west` on a Zephyr build without west"
status: resolved
type: bug
area: [cmake, zephyr, ux]
severity: low
found: 2026-11-05
related: [2248, phase-530]
---

**Home:** [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)
— W7 review round 4 (findings N9, N10), parked as Minor: no in-tree road reaches either.

## Problem

`nros_reconfigure_remedy` (`cmake/NanoRosDiagnostics.cmake`) builds the
command a configure-time FATAL prints to repair the failed build dir
(phase-530 W7, Rulings R54 and R55). Two inputs it takes as given are not
always true:

- **N9 — sysbuild.** Under sysbuild, `CMAKE_BINARY_DIR` is the IMAGE's
  subdir (`<build>/<app>`), not the build dir the user named. The
  `RECONFIGURE` form repairs that image only, and the `PRISTINE` form
  (`west build -p always -d <dir> -b <board> <source>`) would build a
  non-sysbuild image into the subdir.
- **N10 — Zephyr without west.** A defined `ZEPHYR_BASE` is read as "this
  is a west build", so a plain `cmake -B` Zephyr build on a host with no
  west is told to run `west build …`, where `cmake <dir>` would work.

## Why it is parked

No in-tree road reaches either: the tree carries no `sysbuild.conf`, and
every Zephyr road builds with west. `nros build` recovers both cases from
its own build dir regardless.

## Fix sketch

- N9: detect sysbuild (`SYSBUILD` / the parent `APPLICATION_BINARY_DIR`)
  and name the top-level build dir with `--sysbuild`.
- N10: key the west form on west having driven the configure (west passes
  `-DWEST_PYTHON`), not on `ZEPHYR_BASE`.

## Fix (2026-11-12)

`nros_reconfigure_remedy` now asks the two facts it used to assume. Both were
read from the Zephyr 3.7 sources in the store workspace
(`~/.nros/workspaces/zephyr/3.7/zephyr`):

- **N10, west.** West's `scripts/west_commands/build.py` passes
  `-DWEST_PYTHON=<interpreter>` to every cmake it runs, and Zephyr's own
  `cmake/modules/west.cmake` reads "west is here" the same way. The `west build`
  forms are now keyed on `WEST_PYTHON`. A Zephyr build without it gets
  `cmake <dir>`, or "remove `<dir>` and configure it from scratch" when the
  remedy is pristine.
- **N9, sysbuild.** `share/sysbuild/cmake/modules/sysbuild_extensions.cmake`
  configures each image with `-DSYSBUILD:BOOL=True` into
  `${CMAKE_BINARY_DIR}/<image>`. Under `SYSBUILD` the remedy therefore names
  the parent dir. The west pristine form passes `--sysbuild`. Without west, the
  pristine form configures `${ZEPHYR_BASE}/share/sysbuild` with
  `-DAPP_DIR=<source>`.
- Sysbuild forwards every non-`CMAKE_*` cache entry to its images
  (`sysbuild_cache`), so `WEST_PYTHON` reaches an image of a west-driven
  sysbuild, and both questions get the same answer in the image as at the top.

**Test:** `just check cmake-reconfigure-remedy`
(`packages/testing/nros-tests/tests/cmake_reconfigure_remedy.sh`). It makes
one `cmake -P` run of the production function per shape:

| shape | how it relates to the fix |
| --- | --- |
| no Zephyr | unchanged |
| Zephyr with west | unchanged |
| Zephyr without west | the N10 case |
| sysbuild with west | the N9 case |
| sysbuild without west | the N9 case |

Each shape is checked in both kinds, RECONFIGURE and PRISTINE. Against the
pre-fix module, the three changed shapes go red in both kinds (6 of 10), and
the two unchanged shapes stay green.

No in-tree road reaches either case. Every in-tree Zephyr road runs through
west, which passes `WEST_PYTHON`, and none uses sysbuild. Every in-tree remedy
is therefore unchanged.
