---
id: 2558
title: "No CI job runs the multi-edition harness, so Jazzy interop is claimed by nothing; a Humble+Jazzy lane is ruled"
status: resolved
type: test-gap
area: [ci, testing, ros-editions]
severity: medium
found: 2026-11-14
resolved: 2026-11-20
resolved_in: "ci(weekly, issue 2558): a ros-editions lane runs the humble and jazzy harness"
related: [phase-555]
---

Filed by phase-555 W3, ruling C10: "A Humble+Jazzy lane; the Iron claim is
dropped; Jazzy stays codegen-checked until the lane is green."

## What is true now

- `just ros_editions ci <distro>` (`just/ros-editions.just`) builds a
  per-edition ROS image, the edition's fixtures, and runs the harness lanes
  (`ros_editions_{smoke,codegen,bridge,nano_interop,e2e}`) for cyclone, XRCE
  and zenoh. RFC-0058 describes it.
- `git grep ros_editions .github` finds nothing: no workflow runs it, on any
  event. `test-all` deselects the tests (`not binary(~ros_editions)`).
- So the support table's interop claim for Jazzy had no job behind it. As of
  phase-555 W3 the table says Jazzy is **codegen-checked** (the committed
  golden files), and Iron is not claimed.

## Done when

- A workflow runs `just ros_editions ci humble` and `just ros_editions ci
  jazzy` on a schedule (weekly is the brief's suggestion) and on dispatch, on
  a runner with docker. It is not part of the required `CI` check.
- Its first green run moves the Jazzy row in `scripts/gen-support-status.py`
  back to Supported, naming the job.
- RFC-0058's "No workflow runs that composite today" line names the job.

## Unknowns to measure first

The composite was written for a developer machine with the in-tree CLI built
(`just setup-cli`). What a hosted runner must provision (CycloneDDS for the
cyclone fixtures, the XRCE Agent, the host-side message generation for a
non-host edition) has not been measured.

## Measured, 2026-11-20

On a 48-core host with ROS humble sourced, at main `0d01d77056`:

- **The composite had never run from its own front door.**
  `just/ros-editions.just` was the only one of 17 `just` modules without
  `set working-directory := '..'`, so `ci` ran from `just/` and died at
  `source scripts/build/cargo.sh` on every host, for both distros, after the
  image build. `check-just-recipe-paths` resolves recipe paths from the repo
  root, so it called that path present. Fixed, and the gate gained rule 5:
  every module runs from the repo root or carries a written exemption.
- **Humble had no codegen golden.** `ros_editions_codegen` stopped red at
  `[UNMET PRECONDITION] no geometry_msgs golden for edition humble`. The
  humble image ships the same 33 geometry_msgs messages as jazzy (listed from
  `/opt/ros/humble/share/geometry_msgs/msg/`), so the humble golden is
  jazzy's list.
- With both, `just ros_editions ci jazzy` passes in 652 s and
  `ci humble` in 358 s (warm). Every lane is green: smoke, codegen, bridge,
  nano_interop, and 6/6 for each of cyclone, XRCE and zenoh. Each distro
  writes about 16 GB of build output.
- The host side needs docker, a ROS humble install (the fixtures read its
  ament prefix; the edition under test runs in its container), the build
  lane's submodules, the in-tree CLI, and the XRCE Agent from the `nros
  setup` store (`just setup xrce`), which the XRCE lane spawns on the host.
  The cyclone fixtures build the vendored tree; the ROS peers run in the
  edition's image.

## The job

`weekly.yml` job `ros-editions` (lane `ros-editions`), a humble/jazzy matrix
on a hosted ubuntu-22.04 VM (user ruling 2026-11-20: the self-hosted runners
refuse a job the docker socket, and a container job cannot give the harness
`--network host` or host-path `-v` mounts). Weekly and on dispatch, advisory.

It installs ROS humble from the index (`ros-base`, a row added for it,
`ros-interface-packages`, `ros-rmw-zenoh-cpp`), builds the CLI, and runs
`nros setup linux --rmw <r>` for zenoh, XRCE and Cyclone: the RMW sources are
store-first, and the XRCE Agent comes from the store. The first hosted try
used the build lane's provisioner instead and failed in it (a history fetch
the lane does not need, and the CLI called before the job built it).

## First green run

Dispatch 44728059327 (2026-11-20, the job as merged): humble and jazzy both
green in about 29 min each, every lane run (smoke, codegen, bridge,
nano_interop, and 6/6 each for cyclone, XRCE and zenoh), 88 GB free after.
The Jazzy row in `scripts/gen-support-status.py` is Supported, naming the job,
and RFC-0058 names it.
