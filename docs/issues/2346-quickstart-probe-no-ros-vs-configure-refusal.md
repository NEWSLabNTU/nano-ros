---
id: 2346
title: "The checkout probe's quick-start verifier promises a build with no ROS 2 install, but configure refuses an unset AMENT_PREFIX_PATH"
status: open
type: bug
area: probe, cli-configure
related: [issue-2275, issue-2279, issue-2319]
found: 2026-11-07
---

**Home: none yet.** Found from scheduled probe run 42354245272 (main
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
