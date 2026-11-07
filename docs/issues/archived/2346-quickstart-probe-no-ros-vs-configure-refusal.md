---
id: 2346
title: "The checkout probe's quick-start verifier promises a build with no ROS 2 install, but configure refuses an unset AMENT_PREFIX_PATH"
status: resolved
type: bug
area: probe, cli-configure
related: [phase-523, issue-2275, issue-2279, issue-2319, issue-2348]
found: 2026-11-07
purpose: [education, industrial]
resolved_in: "probe-checkout-ros2 branch: the quickstart track runs ros:jazzy and the verifier sources ROS 2"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** (maintainer routing, 2026-11-07). Found from scheduled probe run 42354245272 (main
50b443bd3f), job 128348717207 "book probe — checkout track".

## Symptom

The checkout track (`PROBE_TRACK=quickstart`, image `ubuntu:24.04`, no
`/opt/ros`) runs `scripts/probe/verify-first-node.sh` after the book steps.
It scaffolds `nros new probe_quickstart --workspace` (C++, cyclonedds), then
`nros configure` refuses:

```
Error: configure: `AMENT_PREFIX_PATH` is unset, and `/tmp/probe_quickstart` has no earlier configure that recorded one. Source ROS 2 in this shell (`source /opt/ros/<distro>/setup.bash`, or your overlay's `install/setup.bash`) and run `nros configure` again; later shells need not source it.
```

## Cause

Two promises disagree:

- `verify-first-node.sh`'s header promises that the quick-start workspace
  "builds with two cmake commands and publishes with NOTHING else running —
  no router, no daemon, no ROS 2 install". The track's image has no ROS
  because of that promise.
- Issue 2275's resolution (2026-11-05) ruled that configure keeps refusing an
  unset `AMENT_PREFIX_PATH` (phase-530 Ruling R44, no fallback), and that a
  chapter which does not source ROS is wrong.

`activate.sh` also still says "Setup, codegen and every BUILD do not need
it", which is false since that ruling.

This is not covered by issue 2319's fix in #3262: that fix sources ROS before
the scaffold on the user pages (Tutorial 1 and the starters). The verifier is
probe-owned, not extracted from a page.

## Decision needed

One of:

1. The checkout track provisions ROS 2 (an image with `/opt/ros/<distro>`, or
   an install step), and the verifier sources it before `nros configure`. The
   verifier's "no ROS 2 install" promise and the `activate.sh` line are
   reworded.
2. Configure accepts an unset `AMENT_PREFIX_PATH` when the workspace's
   closure needs no ROS package. This reopens 2275's ruling.

Option 1 follows the standing ruling.

## Acceptance

- The checkout track is green on main.
- `activate.sh`'s warning matches what configure requires.

## Resolution (2026-11-07)

Option 1, which keeps issue 2275's ruling.

- The quickstart track defaults to `ros:jazzy`: noble (the base
  `ubuntu:24.04` was) plus the ROS 2 the book names as a prerequisite.
- `verify-first-node.sh` sources it before the scaffold (which picks its
  edition from it) and before configure, and fails by name on an image
  without one. Its header no longer promises "no ROS 2 install".
- `activate.sh` / `activate.fish` keep a ROS 2 the caller already sourced (any
  edition). Before, they looked only for `/opt/ros/humble` and told a Jazzy
  shell it had none. The no-ROS message no longer says every build works
  without it.

**Verified:** the quickstart track was run (`PROBE_TRACK=quickstart`). It
passed configure and printed `PROBE PASS: scaffolded C++ workspace published
on a pristine host, no router running`. The Rust step then failed on a separate
defect it had been hiding, issue 2348, which is fixed on the same branch.
