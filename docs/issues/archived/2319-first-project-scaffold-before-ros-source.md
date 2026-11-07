---
id: 2319
title: "First Project scaffolds before it sources ROS 2, so a Jazzy host gets a Humble image"
status: resolved
type: bug
area: book, scaffold
purpose: [industrial, education]
related: [phase-523, phase-537, issue-2260]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "book-r2-followup branch: Tutorial 1 and every starter source ROS 2 before they scaffold"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I1, E4: the chapter runs `nros new sensor_ws --board mps2-an385-baremetal --template talker` before any `source /opt/ros/<distro>/setup.bash`. The scaffold prints `ROS 2 edition: humble (the default; source a ROS 2 setup.bash or pass --ros-edition to pick another)` and writes `ros_edition = "humble"`.
- At Run, `nros run` warns `this image speaks ROS 2 humble, and the shell's ROS 2 is jazzy … Set ros_edition = "jazzy"`. The chapter's "on Jazzy, write `jazzy` where they say `humble`" comes too late, because the file is already written.
- Sourcing Jazzy before `nros new` picks `jazzy (the sourced ROS 2, $ROS_DISTRO)`. Both probes found this workaround themselves.

## Next

Move the source step ahead of Scaffold in First Project, or pass `--ros-edition` in the scaffold command. Issue 2260 made the default visible. This issue is about the chapter's order.

## Resolution

The first option. Tutorial 1 (`tutorial/run-a-talker.md`) and every starter
under `book/src/starters/` now open "Scaffold the project" with a
`source /opt/ros/humble/setup.bash` step (probe step 38, on the page's own
track and variant), and say that `nros new` takes the edition from the
sourced ROS 2 and writes Humble with none, so a Jazzy reader sources
`/opt/ros/jazzy/setup.bash` there. A Jazzy host's scaffold then writes
`ros_edition = "jazzy"`, and `nros run` no longer warns about an edition
mismatch. Tutorial 1's first run step dropped its own `source` line, since it
runs in the same shell.

Every probe track still extracts (`python3 scripts/check-book-probe-tracks.py`),
and the extracted `first-project`, `freertos`, `zephyr` and `linux` tracks
parse (`bash -n`), with step 38 between Provision and Scaffold.
