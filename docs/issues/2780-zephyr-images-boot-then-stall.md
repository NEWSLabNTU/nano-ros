---
id: 2780
title: "Zephyr images boot then stall: three merge-runtime regressions in 2cfe9f0094..b13a5d4b18"
status: open
type: bug
area: [zephyr, ci, testing]
severity: high
found: 2026-11-21
related: [2777, 1575, 1551]
---

## What fails

`merge-runtime.yml` run 44768834808 (lane `merge`, main at `b13a5d4b18`), the
zephyr shard (job 136400725703, runner newslab-server-241), has three real
failures. Each is a Zephyr image that boots and then does not get on with
its work:

1. `cli_bringup_zephyr::cli_bringup_zephyr_adapter_shim_boots_native_sim`:
   "native_sim ELF did not print the baked-config boot line — shim contract
   broken", with EMPTY output.
2. `rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`:
   `workspace-probe-seeds (zenoh) — did not reach node registration`. The image
   printed its boot config (domain 13, the test's locator, rmw zenoh) and
   nothing more.
3. `realtime_sched_tasks_e2e::realtime_sched_tasks`: 2 of 2 rows failed.
   - `zephyr/cpp-derived`: node `mrm_emergency_stop_operator` never ticked
     twice, so its derived sched task did not run (issue 1575's image, the one
     issue 1551 ran the heap dry on).
   - `zephyr/cpp-levels`: "too few complete callbacks to judge (0 middle,
     1 lazy)".

## A regression

| run | main | zephyr shard |
| --- | --- | --- |
| 44090176782 | 2cfe9f0094 (2026-11-17) | all three PASS |
| 44721090137 | 0d9429d990 (2026-11-20) | stopped at build (issue 2767), no verdict |
| 44768834808 | b13a5d4b18 (2026-11-20) | all three FAIL |

Read from each run's `merge-runtime-merge-zephyr-junit` artifact.

## Where to look

This is the window of issue 2777 (threadx_linux: an image boots and never
completes its session open), and the symptom has the same shape: an image that
boots and then starves or stalls. A shared root is possible, so the bisect for
2777 should include one of these cells; `cli_bringup` is the cheapest (a single
boot line on native_sim). Suspects, by subject:

1. `6229085857` every image serves the stock node interfaces by default: more
   entities, so more heap and stack at start, which is how issue 1551's image
   failed before.
2. `67d36cb764` a board's executor backing rung is a floor, not a ceiling.
3. `1a896a6722` durations read the monotonic clock (issue 2760).

## Closes when

All three pass again on the merge lane with the cause named here; if the 2777
bisect proves one root for both, they close together.
