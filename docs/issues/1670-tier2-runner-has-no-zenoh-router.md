---
id: 1670
title: "The tier-2 self-hosted runner has no `rmw_zenohd`, so every zenoh runtime
  cell it reaches would skip on a missing router"
status: open
type: tech-debt
area: [ci, testing]
severity: low
found: 2026-10-03
related: [1658, 1158, 1038, 1457, 1482]
---

## What

`run-matrix.yml` (tier 2) runs on the self-hosted
`[self-hosted, linux, nros-qemu, nros-sdk-zephyr, nros-big]` runner. Its
coordinate file holds zenoh cells (`freertos,c,zenoh` among them), but the
runner image carries no `/opt/ros/<distro>` and no `rmw_zenohd`, so once the
lane gets past its build stage (issue 1158) every zenoh runtime cell resolves
no router and reports `[SKIPPED] zenohd not found`. A lane that reaches its
cells and skips all of them is the "verified nothing" shape `_check-skip-budget`
exists to name — it would read as a coverage hole rather than a verdict.

Found while resolving issue 1658, which moved the FreeRTOS C/C++ boot verdict
to the nightly `freertos` job (it already provisions the router through
`nros setup --system --sudo`).

## Direction

An IMAGE fix, never a host `apt install` (issues 1457/1482): the runner image
gains `ros-<distro>-rmw-zenoh-cpp` from the index's declared `[prereq.*]`
closure, the same closure the nightly platform jobs install. The harness pins
the router's paired `libzenohc` itself (`paired_zenoh_library_dir`, issue 0774),
so the image's static `LD_LIBRARY_PATH` does not need to name it.
