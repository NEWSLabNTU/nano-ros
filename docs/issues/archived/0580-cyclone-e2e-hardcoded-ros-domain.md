---
id: 580
title: "Every cyclone interop test named its DDS domain with a literal, so two concurrent runs shared a bus and read the collision as a delivery bug"
status: resolved
resolved_in: phase-340
type: bug
severity: medium
area: testing, rmw
related: [issue-0161, issue-0157]
---

## Symptom

One failure in a tier-2 sweep, in `nros_rmw_cyclonedds_ros2_pubsub_e2e`:

```
=== 117.12.A.1: nros pub → ros2 echo ===
  PASS: ros2 echo captured 'hello-from-nros'
=== 117.12.A.2: ros2 pub → nros sub ===
  FAIL: nros sub captured unexpected payload:
    DATA=hello-from-nros
```

The subscriber was waiting for `hello-from-ros2` and got `hello-from-nros` —
the payload case A.1's **nano-ros publisher** emits. Solo runs of the same suite
passed 17/17, twice.

## Cause

A DDS domain is a shared bus, and every test in the directory named one with a
literal:

| where | domain |
| --- | --- |
| `ros2_pubsub_e2e.sh` | `${ROS_DOMAIN_ID:-117}` |
| `ros2_srv_e2e.sh` | `${ROS_DOMAIN_ID:-118}` |
| `data_roundtrip`, `service_{roundtrip,smoke}`, `pubsub_smoke`, `service_concurrent` (×2) | `99` |
| `feedback_roundtrip` | `88` |
| `session_smoke` | `42` |

So two overlapping runs join the same bus, discover each other's writers, and
the failure presents as wrong data rather than as a collision. That is the
project's own documented rule (cyclone fixture pairs bake DISTINCT domains for
parallel SPDP) not being applied to this suite.

## Reproducing it took the right granularity

Two concurrent copies of the FAILING TEST, both pinned to domain 117, **passed**.
That looked like a refutation and was not: the collision needs one suite's case
A.1 publisher to be alive during the other's case A.2 window, which a single
test cannot produce. Two concurrent full SUITES reproduced it immediately, and
in both copies at once:

```sh
D=packages/rmw/cyclonedds/nros-rmw-cyclonedds/build
( ROS_DOMAIN_ID=117 ctest --test-dir $D --output-on-failure ) &
( ROS_DOMAIN_ID=117 ctest --test-dir $D --output-on-failure ) &
wait
# both: 94% tests passed, 1 failed — "FAIL: nros sub captured unexpected payload"
```

Worth keeping: a failed reproduction at the wrong granularity is not evidence of
absence, and the first attempt here nearly retired a real bug as unexplained.

## Fix

One scheme, three languages — `ROS_DOMAIN_ID` when set (pinning is how you
reproduce by hand), else a per-process domain in `1..=232`:

* `tests/ros2_e2e_common.sh` → `nros_unique_ros_domain_id`, sourced by both
  interop scripts;
* `tests/nros_test_domain.h` → `nros_test_domain()`, included by the eight
  self-contained C++ sites;
* both mirror `nros_tests::unique_ros_domain_id`, rather than inventing a third
  scheme.

Safe for `service_concurrent`, which opens two sessions: every session in ONE
process resolves to the same value. The `ros2_*` binaries, which must MEET a
`ros2` CLI process, keep taking the domain from the environment their shell
script exports.

## Verified

Fixing only the shell half left the suites failing on `service_roundtrip`
instead — the same class, one file over, which is what a per-site fix buys. With
all sites converted:

| experiment | before | after |
| --- | --- | --- |
| suite solo | 17/17 | 17/17 |
| two full suites concurrently | both 16/17 | both 17/17, twice |
