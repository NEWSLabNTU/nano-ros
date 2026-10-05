---
id: 1695
title: "The self-hosted runner IMAGE carries no ROS apt source and no
  `rmw_zenohd`, so tier 2 now stops at `just ci matrix` on every run"
status: resolved
type: tech-debt
area: [ci, infra]
severity: medium
found: 2026-10-05
related: [issue-1670, issue-1457, issue-1482, issue-1158]
---

## What

Issue 1670 made tier 2 refuse to run its zenoh cells without a router
(the private `_require-lane-router` recipe in `just/ci.just`, first step of `just ci matrix` and `just ci
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
paired `libzenohc`, issue 0774). Acceptance: the first line of `just ci matrix`
(`lane tier2: … router: …`) prints the router on the runner, and a tier-2 run reaches its zenoh
cells.

## Resolution

Fixed 2026-10-05 on `fix/1695-tier2-runner-has-ros-router`, in the image
definition — the runner image is built by its operator with
`scripts/ci/runner-container.sh`, and no workflow publishes it, so **the live
runner changes only when its operator rebuilds and restarts it**
(`runner-container.sh <labels> --build`, then `--run`, or `just runner-up`).

**The image** gains a ROS layer carrying exactly one ROS package:
`[prereq.ros-rmw-zenoh-cpp]`, resolved by `prereq-packages.py` for the image's
distro (one input, `NROS_RUNNER_ROS_DISTRO`, default `humble`, which also
becomes `ENV ROS_DISTRO` — the third step of `nros_zenohd_bin`). The repository
comes from `scripts/sdk/ros2-apt-source.sh`, now the one spelling of
packages.ros.org (the distrobox setup calls it too). `AMENT_PREFIX_PATH` is
deliberately NOT set: that would make the prefix's message packages
discoverable to every build on the runner, a different decision from "has a
router", so issue 0368's ROS-less cyclone road still stands. The image's own
acceptance runs at BUILD time with the repo's `scripts/dev/zenohd.sh`:
`nros_zenohd_bin` must resolve and `nros_router_exec` must still be running
3 s later.

**Resolving was not running.** Measured in the image (ROS_DISTRO set, nothing
sourced): `rmw_zenohd` has no RUNPATH, needs `librmw`, `librcutils` and
`libament_index_cpp` from `<prefix>/lib` besides `libzenohc`, and with those it
aborts on `Environment variable 'AMENT_PREFIX_PATH' is not set or empty`. So
the step-3 resolution — which exists for exactly an unsourced host — found
routers that could not start, on the runner and on any unsourced dev box.
Both launchers now start the router through its OWN prefix's `setup.bash`, for
that process only, when the caller has not sourced it:
`nros_router_env_exec` in `scripts/dev/zenohd.sh` (used by
`nros_router_exec` and the qemu-baremetal rtic recipe) and
`unsourced_prefix_setup` in the nros-tests router fixture.

Two layer-order facts were measured, not chosen: the ROS key is a BINARY
OpenPGP key (saved as `.asc`, apt refuses it with `NO_PUBKEY
F42ED6FBAB17C654`), and the repository must precede the `[python.*]` layer —
behind jammy universe alone that layer installs `python3-catkin-pkg` 0.4.24,
and the router's dependencies then pull packages.ros.org's
`python3-catkin-pkg-modules` 1.1.1, which dpkg refuses to overwrite it with.

**Measured** — image built locally as `nano-ros-runner:issue1695`, run as its
own `runner` user (uid 1001) under `--cap-drop ALL --security-opt
no-new-privileges`:

| | before | after |
| --- | --- | --- |
| `nros_zenohd_bin` | (no ROS in the image) | `/opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd` |
| `nros_router_exec tcp/127.0.0.1:17695` | — / with the ROS layer but main's launcher: exit 127, `libament_index_cpp.so: cannot open shared object file` | alive after 3 s, port accepts connections |
| `libzenohc` mapped | — | `/opt/ros/humble/opt/zenoh_cpp_vendor/lib/libzenohc.so` (the only one in the image — paired, issue 0774) |

Build-time negative control: the same image built with main's `zenohd.sh`
fails its acceptance step (rc 127). Host, nros-tests fixture
`ZenohRouter::start_unique()` with ROS unsourced: before, `exit status: 127 …
libament_index_cpp.so`; after, `router up on 127.0.0.1:42811`. Unit
`an_unsourced_router_runs_in_its_prefix_environment`.

**Not measured:** a tier-2 run on the real self-hosted runner — the image must
be rebuilt there first, so the issue's acceptance (`lane tier2: … router: …`
printed by `ci::_require-lane-router`, and a run reaching its zenoh cells)
is the operator's to observe; `ci::_require-lane-router` itself was not run
inside the image (`just` lives in the runner's volume, not the image).
