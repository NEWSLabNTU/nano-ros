---
id: 1670
title: "The tier-2 self-hosted runner has no `rmw_zenohd`, so every zenoh runtime
  cell it reaches would skip on a missing router"
status: resolved
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

## Resolution

Fixed 2026-10-05 on `fix/1670-tier2-runner-router`, by the second of the two
roads: the lane now RESOLVES the router itself and refuses, loudly, to run its
zenoh cells without one. The image half is
[issue 1695](../1695-tier2-runner-image-lacks-ros-router.md).

**Why not "the same step the nightly lanes use".** Those jobs run `nros setup
--system --sudo` inside `nano-ros-ci:humble` (a ROS base image, root). Tier 2
runs on the self-hosted runner, which is a non-root `--cap-drop ALL` container
built `FROM ubuntu:22.04` with no ROS apt source — the step cannot install
there and must not install on the host (issues 1457/1482). So the sharable
piece is the RESOLVER, not the install.

**What changed** (`just/ci.just` only; no workflow change):

* `ci::_require-lane-router <lane>` reads the lane's own coordinate file
  (`nros_lane_coords_file`, the one the build and the run narrow by); a lane
  with no `zenoh` coordinate asks nothing. Otherwise it resolves the router with
  `nros_zenohd_bin` (`scripts/dev/zenohd.sh` — the resolver the harness's
  `ros_zenohd_path`, `just doctor` and `runner-doctor.sh nros-ros2` share) and
  exits 1 naming every zenoh coordinate that would have skipped and where the
  router must come from.
* It is the FIRST step of `_matrix-run` (`just ci matrix`, which
  `run-matrix.yml` runs) and of `matrix-nightly`, ahead of the freshness gate.

**Measured** — the workflow's own command, `just ci matrix`:

| environment | result |
| --- | --- |
| router unresolvable (`AMENT_PREFIX_PATH= ROS_DISTRO= NROS_RMW_ZENOHD=`, the runner's shape) | rc 1 at `_require-lane-router`: "lane tier2 runs 8 zenoh coordinate(s) and this host resolves no rmw_zenohd", listing `baremetal,rust` … `zephyr-cortex-m,c` |
| `ROS_DISTRO=humble` (this host) | `lane tier2: 8 zenoh coordinate(s); router: /opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd`, then on to `_lane-gate` |
| `ci::_require-lane-router tier2-nightly` with ROS | 24 zenoh coordinates, router resolved |

BEFORE: the same command on the runner would have built everything and let
each of those 8 cells `skip!("zenohd not found")`.

Not measured: the self-hosted runner itself (not reachable from here). Until
issue 1695 lands, tier 2 fails at this step on that runner — the intended
loud answer in place of eight silent skips.
