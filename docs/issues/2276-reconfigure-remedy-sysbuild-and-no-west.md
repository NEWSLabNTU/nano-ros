---
id: 2276
title: "The re-configure remedy assumes a non-sysbuild west build: it names the image subdir under sysbuild, and `west` on a Zephyr build without west"
status: open
type: bug
area: [cmake, zephyr, ux]
severity: low
found: 2026-11-05
related: [2248, phase-530]
---

**Home:** [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)
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
