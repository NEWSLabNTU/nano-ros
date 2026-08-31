---
id: 947
title: "NuttX has two ROS-edition surfaces that cannot express each other, and `jazzy` is unreachable on one"
status: resolved
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

## Resolved — 2026-08-31

**One vocabulary per integration: a Kconfig choice for the menu, a derived
string for the consumers.** That is not a new idiom — `integrations/nano-ros/
Kconfig.projbuild` already does exactly this one symbol higher up, for
`NROS_RMW`. The edition was the outlier.

```kconfig
choice
    prompt "ROS 2 edition"
    default NROS_ROS_HUMBLE
config NROS_ROS_HUMBLE
    bool "Humble"
config NROS_ROS_IRON
    bool "Iron"
config NROS_ROS_JAZZY
    bool "Jazzy"
endchoice

config NROS_ROS_EDITION
    string
    default "humble" if NROS_ROS_HUMBLE
    default "iron"   if NROS_ROS_IRON
    default "jazzy"  if NROS_ROS_JAZZY
```

Both NuttX lanes now read that one symbol. The CMake shell's `else()` branch is
reachable for the first time — the string symbol it always read now exists. The
Makefile derives the cargo feature (`ros-$(NROS_ROS_EDITION)`) instead of
starting at `ros-humble` and rewriting itself with a `filter-out` per edition,
so **adding an edition needs no Makefile edit at all** — which is precisely why
`jazzy` had been missing for two phases.

The ESP-IDF side got the same treatment. Its `NROS_ROS_EDITION` was a FREE-FORM
string prompting `"(humble | iron)"`, which both omitted jazzy and would have
accepted any typo, surfacing as a nonexistent cargo feature rather than a
Kconfig error.

Both CMake shells now call `_nros_resolve_ros_edition()` (phase-405 W3) instead
of defaulting to a literal, so a bad `sdkconfig` value fails **with the value
that caused it** rather than passing through.

The Makefile's empty-value case is a hard `$(error)`, not a default. The Kconfig
choice always yields a value when the fragment is sourced, so an empty one means
it is not sourced — and quietly picking humble there is the RFC-0056 wire
mismatch this whole class is about.

**Measured**, by evaluating the real Make fragment rather than reading it (the
`patsubst "%",%` quote-stripping is the part that would silently produce
`ros-"jazzy"`):

```
CONFIG_NROS_ROS_EDITION='"jazzy"'  ->  features=std ros-jazzy platform-nuttx
CONFIG_NROS_ROS_EDITION='"iron"'   ->  features=std ros-iron platform-nuttx
unset                              ->  *** CONFIG_NROS_ROS_EDITION is unset ... Stop.
```

**The gate's glob is widened to `integrations/`, which is what closing this
issue was for.** `check-feature-set-ssot.sh` now covers it, with exactly one
allowlisted shape — the Kconfig mapping stanza `default "<ed>" if NROS_ROS_<ED>`,
since Kconfig cannot call a cmake function and each integration needs its own
two-line mapping. Proven red-capable inside the newly covered scope: a bare
`NROS_ROS_EDITION := humble` planted in `integrations/nuttx/Makefile` gives
rc=1, clean gives rc=0. The selftest exercises both directions of the new
allowlist on the normal path.

**Not verified:** no NuttX kconfig+Make build or ESP-IDF build was run — neither
toolchain is available here. The Make derivation was evaluated directly and the
Kconfig structure checked, but an end-to-end NuttX image was not produced.
