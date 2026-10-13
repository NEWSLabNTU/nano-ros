---
id: 1853
title: "live-peer's ci-image board job: `rmw_zenohd` exits 1 before listening, and the only stderr is an rcutils logging error"
status: resolved
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-12
related: [1847, 1811, 0774]
---

## Measured

The live-peer regression run
[38291960490](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38291960490)
(`workflow_dispatch`, main at `a310e0305`) was the first run after #2057 to get
the ci-image board job past the fixture build (the issue-1847 `--frozen` stop).
Job 114954544995, "rows whose board is NOT this runner (ci image)", container
`ghcr.io/newslabntu/nano-ros-ci:humble`. The provisioning, setup, fixture-build
and nextest-provisioning steps all passed. Step "Run the board cells with a
recorded PASS" then failed:

```
FAIL [   2.131s] (1/1) nros-tests::pubsub_freertos_ros2_interop_e2e nros_freertos_mps2_publisher_reaches_ros2_topic_echo
panicked at packages/testing/nros-tests/tests/pubsub_freertos_ros2_interop_e2e.rs:125:29:
[UNMET PRECONDITION] zenohd failed to start on 7930: Process failed: zenohd exited before listening on tcp/0.0.0.0:7930 with exit status: 1: logging system isn't initialized: call to rcutils_logging_console_output_handler failed.
Summary [   2.134s] 1 test run: 0 passed, 1 failed, 1 skipped
```

So the cell never ran. This is a precondition failure, not a verdict on the
FreeRTOS publisher.

## What the text says, and what it does not

`wait_for_router_ready` (`packages/testing/nros-tests/src/fixtures/zenohd_router.rs`)
reports the router's whole stderr. The whole stderr is the rcutils line.
That line is what rcutils prints when something LOGS before
`rcutils_logging_initialize` has run. The router hit an error and tried to report
it through a logging system it had not set up, so the real reason it exited is
lost. Candidates, none measured:

- The listen endpoint or its config could not be opened.
- A `libzenohc.so` mismatch through the loader (issue 0774 shape). That one
  usually SEGVs rather than exiting 1.
- Something in the container's HOME/ROS log directory (`/github/home`).

It is NOT 1847: the fixture build passed. It is NOT the
`_require-lane-router` "cannot locate rmw_zenohd" shape either: the router was
found and started, then exited.

## What would close it

1. Reproduce in `ghcr.io/newslabntu/nano-ros-ci:humble`. Start the router the
   way `ZenohRouter::start_on("0.0.0.0", 7930)` does, with its env, and recover
   the real error. Initialising rcutils logging first, or reading the router's
   own log, should show it.
2. Fix the cause in the image or the fixture.
3. Make `wait_for_router_ready` surface more than an uninitialised-logging
   line, so the next exit names its cause.
4. Acceptance: a live-peer dispatch where this cell produces a verdict.

## Cause (measured 2026-10-13)

It was a partial apt upgrade, not anything in the harness or the image. Both
runs pulled the same `nano-ros-ci:humble` digest (`sha256:e2e1d193…`). The
passing run 38251106285 printed `ros-humble-rmw-zenoh-cpp is already the newest
version (0.1.9…)`. The failing run 38291960490, a day later, unpacked
`ros-humble-rmw-zenoh-cpp (0.1.11-1jammy.20261011…) over (0.1.9…)`, released
on 2026-10-11, and did NOT touch `ros-humble-zenoh-cpp-vendor`. bloom's
`Depends: ros-humble-zenoh-cpp-vendor` carries no version, so apt had no
reason to move it. `[prereq.ros-rmw-zenoh-cpp]` named only the router, so
`nros setup --system` asked apt for exactly one of the two.

Reproduced on a workstation without installing anything: `apt-get download`
both 0.1.11 debs, `dpkg-deb -x` them into a scratch dir, and start the 0.1.11
`rmw_zenohd` with the harness's `ZENOH_CONFIG_OVERRIDE`.

- Against the installed 0.1.9 `libzenohc.so`, it exits 1 with exactly
  `logging system isn't initialized: call to rcutils_logging_console_output_handler failed.`
- Against the 0.1.11 `libzenohc.so`, it runs (killed by `timeout`).

## Fix

- `[prereq.*] ros_package` takes a string OR a list. The row is now
  `ros_package = ["rmw_zenoh_cpp", "zenoh_cpp_vendor"]`, so every install
  command derived from it names both, and apt moves them as a pair. Readers:
  `sdk_index.rs` (`PrereqDep::packages_for`) and
  `scripts/sdk/prereq-packages.py`, each with a test. `nros setup --system`
  now prints `... ros-humble-rmw-zenoh-cpp ros-humble-zenoh-cpp-vendor`.
- `just ci provision-zenohd` installs the list as separate arguments.
- `wait_for_router_ready` adds a diagnosis when the router's whole stderr is
  the rcutils line. That line hides the real cause, and the commonest cause
  is this release mismatch.

Acceptance is unchanged: a live-peer dispatch where this cell produces a
verdict.

## Resolution (2026-10-13)

Accepted on live-peer 38321018350, dispatched on `main` (`05a52e7cdd`) after
PR #2082. The ci-image board job, "rows whose board is NOT this runner (ci
image)", succeeded, and its stage report reads `VERDICT: cells ran and
passed`. The `pubsub_freertos_ros2_interop_e2e` cell that this issue
recorded as never running produced a verdict.
