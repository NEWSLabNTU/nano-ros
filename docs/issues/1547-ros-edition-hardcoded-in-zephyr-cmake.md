---
id: 1547
title: "`ros-humble` is hardcoded 11 times in `zephyr/CMakeLists.txt`, outside the one place the edition is meant to be chosen"
status: open
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
