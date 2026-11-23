---
id: 2889
title: "live-peer: the Zephyr Cortex-M publisher no longer reaches `ros2 topic echo`"
status: open
type: bug
area: [zephyr, testing, rmw-zenoh]
severity: medium
found: 2026-11-23
related: [phase-590]
---

## What fails

`live-peer.yml` (run 45110360962, 2026-11-23, main `856de6eb48`), both jobs red:

```text
FAIL [  73.610s] (1/1) nros-tests::pubsub_zephyr_cortex_m_ros2_interop_e2e nros_zephyr_cortex_m_publisher_reaches_ros2_topic_echo
```

The job `rows whose board IS this runner` failed at `Build the fixtures those rows resolve`. The
lane was green on 2026-11-20 (`0a3e2755f3`) and red on 11-21, 11-22 and 11-23.

## Next

Read the job logs for the build error and the test's own output, then bisect between
`0a3e2755f3` and `3b6959ca7f` if it is a code regression.

## Acceptance

`live-peer` green in a scheduled run. A release blocker:
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.
