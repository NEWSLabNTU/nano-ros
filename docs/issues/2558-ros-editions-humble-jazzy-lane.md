---
id: 2558
title: "No CI job runs the multi-edition harness, so Jazzy interop is claimed by nothing; a Humble+Jazzy lane is ruled"
status: open
type: test-gap
area: [ci, testing, ros-editions]
severity: medium
found: 2026-11-14
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
