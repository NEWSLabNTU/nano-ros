---
id: 1853
title: "live-peer's ci-image board job: `rmw_zenohd` exits 1 before listening, and the only stderr is an rcutils logging error"
status: open
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
