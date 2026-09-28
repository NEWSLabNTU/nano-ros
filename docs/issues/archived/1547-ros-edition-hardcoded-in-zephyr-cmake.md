---
id: 1547
title: "`ros-humble` is hardcoded 11 times in `zephyr/CMakeLists.txt`, outside the one place the edition is meant to be chosen"
status: resolved
resolved: 2026-09-28
type: bug
area: [build, zephyr]
severity: medium
found: 2026-09-28
related: [phase-472, 0934]
---

## What happens

VERIFIED: `zephyr/CMakeLists.txt` names `ros-humble` 11 times, in feature strings;
`zephyr/cmake/nros_generate_interfaces.cmake:133` defaults the edition to
`"humble"`; `packages/rmw/zenoh/nros-rmw-zenoh-staticlib/CMakeLists.txt:55` names
it too.

`check-feature-set-ssot.sh`'s edition arm states that the ROS edition is chosen in
exactly one place, and reads `cmake/`, the root `CMakeLists.txt`, the two API
`CMakeLists.txt` and `integrations/` — never `zephyr/`. The same line pasted into
`cmake/NanoRosEntry.cmake` fails the gate.

## Why it matters

A Zephyr image built for any edition other than humble takes humble's feature
set from these strings regardless of what was selected.

## Fix

Route the Zephyr sites through the one edition resolver; widen the gate to
`zephyr/**` and `packages/**/CMakeLists.txt` (phase-472 W5).

## Resolution

Fixed 2026-09-28 on branch `fix/1547-1548-zephyr-ci`. Every site now goes
through `_nros_resolve_ros_edition()` (`cmake/NanoRosRosEdition.cmake`):

- `zephyr/CMakeLists.txt` resolves once into `_nros_edition_feature`, beside
  `_nros_trace_suffix`, and all nine nros-c / nros-cpp feature strings use it.
  One variable for the same reason as the trace suffix: the two cargo units
  come from one nros-node graph and must agree.
- `zephyr/cmake/nros_generate_interfaces.cmake` includes the resolver at file
  scope and calls it instead of the `"humble"` fallback, so it also gains the
  `NANO_ROS_ROS_EDITION` rung and the unknown-edition refusal.
- `packages/rmw/zenoh/nros-rmw-zenoh-staticlib/CMakeLists.txt` resolves
  before `corrosion_import_crate`.
- `check-feature-set-ssot.sh`'s edition arm reads `packages/**/CMakeLists.txt`,
  `packages/**/*.cmake` and the cmake files under `zephyr/` (phase-472 W5).
  Not all of `zephyr/**`: the one other hit is Kconfig help text naming the
  Debian package `ros-humble-cyclonedds`, which selects nothing.

**Measured** (configure-only, native_sim/native/64, examples/zephyr/{c,cpp}/talker
against this worktree's module, zenoh / xrce / cyclonedds; feature strings read
from build.ninja and from a `--trace-expand` of the module, since the Cyclone
examples fail later in the APP's own CMakeLists either way):

- pre-fix, `-DNANO_ROS_ROS_EDITION=jazzy`: every cargo `--features` string had
  `ros-humble` while the interface codegen emitted `"ros_edition": "jazzy"` in
  the same image, which is the defect.
- post-fix, default: feature strings byte-identical to origin/main for all six
  configurations; codegen still humble.
- post-fix, jazzy: every string carries `ros-jazzy`; codegen jazzy.
- staticlib: no in-tree cmake consumer reaches it (zenoh links as an umbrella
  rlib, so its header comment about a root `add_subdirectory` is stale).
  Standalone with a stubbed `corrosion_import_crate` it gives
  `ros-humble;std;platform-posix` by default and `ros-jazzy;...` with jazzy.
- gate: green on the fixed tree; restoring the three pre-fix files turns it red
  naming exactly those three.

**Not verified:** no Zephyr image was compiled or run. Zephyr has no Kconfig
edition choice, so the selection path is `-DNANO_ROS_ROS_EDITION` on the west
command line, or setting it before `find_package(Zephyr)`. The
zephyr-lang-rust lane was not touched.
