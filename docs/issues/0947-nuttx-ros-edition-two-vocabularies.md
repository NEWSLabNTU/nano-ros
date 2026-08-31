---
id: 947
title: "NuttX has two ROS-edition surfaces that cannot express each other, and `jazzy` is unreachable on one"
status: open
area: build
severity: medium
related: [0946, phase-405, RFC-0056]
found: 2026-08-31
---

# One platform, two lanes, three vocabularies

NuttX selects its ROS edition twice, in spellings that cannot represent the same
set of values:

* **CMake lane** — `integrations/nuttx/CMakeLists.txt:18` reads
  `CONFIG_NROS_ROS_EDITION`, a **string** symbol that `integrations/nuttx/Kconfig`
  **does not declare** (only `integrations/nano-ros/Kconfig.projbuild:46` does,
  and that is the ESP-IDF tree). So the `else()` branch at
  `nuttx/CMakeLists.txt:20-22` is unreachable from NuttX's own Kconfig, and that
  lane is permanently `humble`.
* **Make lane** — `integrations/nuttx/Kconfig:61-70` declares a **bool choice**,
  `NROS_ROS_HUMBLE` / `NROS_ROS_IRON`, read only by
  `integrations/nuttx/Makefile:93,110`, which map it to the **cargo feature**
  spelling `ros-humble` / `ros-iron`.

`jazzy` has been supported since phase-304 W2b (`rosidl-codegen/src/types.rs`),
and there is no `NROS_ROS_JAZZY` symbol anywhere — `git grep NROS_ROS_JAZZY`
returns nothing. So on NuttX, jazzy is unreachable by construction on the Make
lane and unreachable by omission on the CMake lane.

Adjacent, same class: `integrations/nano-ros/Kconfig.projbuild:47` prompts
`"ROS 2 edition (humble | iron)"`, which has been missing jazzy since phase-304.

## Why this blocks a gate

`scripts/check-feature-set-ssot.sh` was rewritten in phase-405 W3 to match the
bare word `humble` (it previously grepped `ros-humble` and matched **none** of
the six cmake sites it existed to catch). Its glob deliberately stops at
`cmake/**` + the root and api `CMakeLists.txt`, and **excludes `integrations/**`**
— because widening it now would flag `integrations/nuttx/Makefile`'s literals,
which cannot be fixed by calling the shared resolver: the Make lane has no cmake
and no shared vocabulary to call into.

Landing a red nobody can turn green is how a gate gets switched off. So the glob
stays narrow, with a comment naming this issue, and **widening it is part of
closing this one**.

## Work

1. Decide the NuttX edition vocabulary — one spelling per platform. Most likely
   a string symbol in `integrations/nuttx/Kconfig` mirroring ESP-IDF's, with the
   Makefile deriving `ros-<edition>` from it rather than from a bool choice.
2. Add `jazzy` wherever the choice is enumerated, including the ESP-IDF prompt.
3. Widen `check-feature-set-ssot.sh`'s glob to `integrations/**` and remove the
   scope comment that points here.

Needs a NuttX kconfig+Make build to verify; found by reading, not by building.
