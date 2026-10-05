---
id: 1695
title: "The self-hosted runner IMAGE carries no ROS apt source and no
  `rmw_zenohd`, so tier 2 now stops at `just ci matrix` on every run"
status: open
type: tech-debt
area: [ci, infra]
severity: medium
found: 2026-10-05
related: [issue-1670, issue-1457, issue-1482, issue-1158]
---

## What

Issue 1670 made tier 2 refuse to run its zenoh cells without a router
(`just ci::_require-lane-router`, first step of `just ci matrix` and `just ci
matrix-nightly`) instead of letting all 8 of them skip. That turns the lane's
silent coverage hole into a loud, named failure — and on the current
self-hosted runner it will fire on every run, because the runner cannot
resolve `rmw_zenoh_cpp/rmw_zenohd`.

The router has to come from the IMAGE (issues 1457/1482: a self-hosted runner
is a container, `--cap-drop ALL`, non-root, so nothing inside it can install a
system package, and a host install is a machine nobody can reproduce).
`scripts/ci/runner-container.sh` generates that image `FROM ubuntu:22.04` with
a fixed `PREREQ_KEYS` list resolved through `prereq-packages.py`. Adding
`ros-rmw-zenoh-cpp` to that list is not enough on its own:

* `ubuntu:22.04` has no ROS 2 apt source, so `ros-humble-rmw-zenoh-cpp` has
  no candidate. The nightly platform jobs get one for free from
  `ghcr.io/newslabntu/nano-ros-ci:humble` (`FROM ros:humble-ros-base`).
* `runner-container.sh`'s own header says "THE RUNNER IS NOT MISSING ROS. A
  ROS-less runner is the design" (for the cyclone msg->IDL road, issue 0368).
  That decision predates tier 2 holding zenoh cells and has to be revisited,
  not quietly overridden.
* `nros_zenohd_bin` (and the harness's `ros_zenohd_path`) resolve through
  `NROS_RMW_ZENOHD`, `AMENT_PREFIX_PATH` or `ROS_DISTRO` — the runner's
  environment must name one of them, which today it does not.

## Direction

Either base the runner image on the ROS image the nightly CI image already
uses (or add the ROS apt source in the generated Dockerfile, from the index
rather than a hand-written line), add `ros-rmw-zenoh-cpp` to `PREREQ_KEYS`,
and set `ROS_DISTRO` in the image; or give tier 2 a runner labelled
`nros-ros2` (`runner-doctor.sh nros-ros2` already checks the router and its
paired `libzenohc`, issue 0774). Acceptance: `just ci::_require-lane-router
tier2` prints the router on the runner, and a tier-2 run reaches its zenoh
cells.
