---
id: 1477
title: "`provision-zenohd` prints `error: ... failed with exit code 78` in every
  nightly — an honest lane-skip wearing a failure's clothes, attributed nowhere"
status: resolved
type: bug
area: [ci, tooling]
severity: low
found: 2026-09-23
resolved_in: "phase-466 follow-up (this commit)"
related: [1464, 1226, 1070, 1025, 0599, 0650, 1482]
---

## Symptom

Every scheduled tier-2 run, in the `just setup tier2-nightly` step:

```
NROS_LANE_SKIP: not root: ros-humble-rmw-zenoh-cpp not installed. ...
lane skipped: not root: ros-humble-rmw-zenoh-cpp not installed. ...
error: recipe `provision-zenohd` failed with exit code 78
native setup: rmw_zenohd not provisioned — zenoh lanes will SKIP
```

No issue named it. It stayed invisible because the lane was already red, and a
red lane reports its FIRST failure only — the 1025 / 1070 pattern.

## What 78 is, and whether it mattered

**It is the lane-skip protocol, and the run was fine.** Measured from nightly
run 35821404524 (2026-09-23), job-log lines 858–915:

* `nros_lane_skip` (`scripts/build/lane-skip.sh`, issue 0599) gives a lane a
  THIRD verdict and exits **78 — sysexits' `EX_CONFIG`**. A chosen code, not a
  crash.
* `just/ci.just`'s `provision-zenohd` took its "workstation" branch (the
  self-hosted runner is not root) and called it.
* `just/native.just`'s `setup` INTERPRETED it — the caller
  `check-lane-skip-interpreters` already declares for this producer — 1.2 ms
  later, and the step ran on to `native lane provisioned.` The run's real
  failure was three steps later in `just build tier2-nightly`, unrelated.

The lane also genuinely cannot have a router and does not need one: RFC-0075
ships none, it comes from ROS, and `matrix-nightly` runs on a bare self-hosted
runner (`[self-hosted, linux, nros-qemu, nros-sdk-zephyr, nros-big]`, no
container) whose same log says `activate.sh: /opt/ros/humble/setup.bash not
found`. The zenoh cells report `[SKIPPED:capability]`, which
`check-zenohd-router-skips` enforces.

The nearby `setup: NOT fatal — provisioning continues, and nothing here installs
system packages` is a DIFFERENT and also-correct abstention, from `nros setup
--system` several steps earlier — not this recipe.

## What was actually wrong

Only the reporting, at one site. `just` announces every non-zero recipe exit
itself, from the SUB-process, before the calling shell can look at the code.
Nothing in the recipe can suppress that and nothing should — the recipe did exit
non-zero. What it is not is a failure.

The fixture fan-out never has this problem: `build-test-fixtures-leaves`
redirects each lane's `just` into a per-lane log and prints its own
`== <lane> == SKIPPED (reason)`. Of the 16 producers in
`check-lane-skip-interpreters`' `INTERPRETED` map, `provision-zenohd` is the
**only interpreted one whose sub-`just` writes straight to the console**; the
rest sit behind that driver or are invoked directly by a person. One site, not a
class.

## Fix

`just/native.just`'s `setup` now ATTRIBUTES the line above it on the 78 path:
what 78 is, that the `error:` line is expected, which gate binds this producer
to this caller, and why this lane has no router. Exit code, protocol, skip
markers and gate are unchanged.

Making the message quieter was considered and rejected: `[no-exit-message]` on
the recipe would also swallow the announcement of a genuine failure, and the
repo's rule is that a reported skip is honest and a silent one is not.

## Not fixed here

The tier-2 self-hosted runner has no ROS install, so every zenoh interop cell in
the pairwise cover skips. Whether that is intended or a runner-provisioning gap
like the `catkin_pkg` one belongs to whoever owns that box — installing
`ros-humble-rmw-zenoh-cpp` there needs root on a self-hosted runner, which no
agent has.

## CORRECTED — 2026-09-24, issue 1482

The last paragraph frames the remaining question as ownership of a box:

> Whether that is intended or a runner-provisioning gap like the `catkin_pkg`
> one belongs to whoever owns that box — installing `ros-humble-rmw-zenoh-cpp`
> there needs root on a self-hosted runner, which no agent has.

Two corrections, and the first is what the `catkin_pkg` comparison was reaching
for.

**A self-hosted runner here is a container**, started by
`scripts/ci/runner-container.sh` from an image generated out of
`nros-sdk-index.toml`. So "needs root on the runner" is not the obstacle it
reads as: the image build HAS root, and the running container deliberately does
not — `--cap-drop ALL --security-opt no-new-privileges`, non-root user — which
means a root-owned dependency has exactly one legitimate producer and it is not
a person typing `sudo apt` on the workstation. That is the shape 1482 fixed for
the `[python.*]` layer, and this issue's `catkin_pkg` analogy was right about
the class while both issues had the remedy pointing at the host.

**But the zenoh half is NOT the same fix**, and it should not be waved through
on the analogy. `catkin_pkg` is four pure-python modules Ubuntu already packages
(measured: `ubuntu:22.04` universe carries every one), added to an image that
stays `FROM ubuntu:22.04`. A router means `ros-humble-rmw-zenoh-cpp`, which
means the ROS 2 apt archive in the runner image and a base that carries ROS —
a real widening of what that image trusts and a label (`nros-ros2`) this runner
does not claim. RFC-0075 ships no router precisely so it comes from a ROS
install, so the choice is an ROS-carrying runner image or accepting
`[SKIPPED:capability]` on the zenoh cells.

So: still not fixed here, and still a decision rather than an oversight — but
the decision is about the IMAGE, not about who has root on a box.
