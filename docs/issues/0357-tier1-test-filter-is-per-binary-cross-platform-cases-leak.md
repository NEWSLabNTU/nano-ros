---
id: 0357
title: "Tier 1's test filter excludes per-BINARY, so cross-platform cases inside generically-named binaries still run"
status: open
severity: P2
area: testing
created: 2026-07-31
refs:
  - RFC-0061
  - phase-318 (W4)
  - issue 0196
---

## Summary

`just ci` (tier 1) is defined as native-only, and its FIXTURE gate scopes
correctly — a tree with no ThreadX/NuttX/FreeRTOS/Zephyr build dirs at all runs
tier 1 without a single complaint about them (measured 2026-07-31).

Its TEST selection does not. `scripts/test/lane-filter.sh native` emits
binary-level exclusions:

```
not binary(~esp32)  not binary(~freertos)  not binary(~fvp)  not binary(~nuttx)
not binary(~px4)    not binary(~qemu)      not binary(~stm32) not binary(~threadx)
not binary(~zephyr)
```

That only works when a platform's tests live in a binary *named after it*. The
matrix consumers do the opposite: one generically-named binary holds every
platform's cases (`rtos_e2e`, `entry_e2e`, `realtime_tiers_e2e`). Those names
match no token, so the whole binary runs — on a host with none of those
platforms' fixtures.

## Measured

A tier-1 run on 2026-07-31 (`just ci`, `NROS_TEST_SCOPE=native`) executed 1322
tests, of which 88 distinct tests failed. **53 of the 88 name a non-native
platform** and should never have been selected:

| binary | cross-platform failures |
| --- | --- |
| `nros-tests::rtos_e2e` | 24 |
| `nros-tests::entry_e2e` | 11 |
| `nros-tests::realtime_tiers_e2e` | 10 |
| `nros-tests::logging_smoke` | 3 |
| `nros-tests::native_api` | 2 |
| `nros-tests::multihost_e2e` | 1 |
| `nros-tests::cmake_platform_matrix` | 1 |
| `nros-tests::cli_bringup_platformio` | 1 |

Examples: `entry_e2e entry_matrix::case_01_threadx_linux_c`,
`case_05_freertos_cpp`.

Note `rtos_e2e` — a binary that is *entirely* cross-platform and contains no
family token in its name, so no exclusion can reach it.

## Why this matters

It defeats the point of the tier. Tier 1 exists so a developer can run something
affordable per task; a lane that reports 53 failures for platforms the host has
no fixtures for is a lane whose reds get ignored, which is the exact second-order
cost RFC-0061 §"The second-order cost" is written about. It also makes tier 1
useless as a pre-push gate: nobody can tell a real native regression from the
noise without hand-classifying 88 names.

## The gate was narrower than the rule it enforced

`ci_lane::tests::lane_filter_tokens_cover_every_non_native_platform` asserts that
the filter emits a `binary(~token)` exclusion for every non-native `PlatformId`.
It passes, and it always would have — it checks that the *tokens* are complete,
never that the *selection* is. Platform coverage is not per-binary, so a complete
token set does not imply a correct lane.

This is the issue-0196 class ("gates whose coverage is narrower than the rule they
enforce"), in the work that introduced the rule. Filed by the author.

## Fix sketch

The selection already exists and is per-CELL, not per-binary:
`ci_lane::cells(CiLane::Tier1)`. The filter should be derived from it rather than
from `PlatformId` name tokens. Two candidate shapes:

1. **Emit test-name exclusions as well as binary ones.** The matrix consumers name
   their cases after the cell (`case_01_threadx_linux_c`), so
   `not test(~threadx_linux)` is derivable from the same table. Cheap, and it
   composes with what is there.
2. **Have the matrix consumers skip out-of-lane cells themselves** — read
   `NROS_TEST_SCOPE` in the parametrized harness and `skip!` a cell whose platform
   is not in the lane. More invasive, but it puts the decision where the cell is
   known, and a skip is honest where an exclusion is invisible.

(2) is probably right long-term; (1) unblocks tier 1 now.

Whichever ships, the anti-rot test must move with it: assert that the resolved
selection contains no non-native CELL, not that the token list is complete.

## Not in scope

The remaining 35 failures are not classified here — several look
environment-dependent (XRCE agent, `zenohd`, a ROS 2 peer for interop) rather than
code reds, and separating those needs its own pass. This issue is only about the
53 that tier 1 should not have run.
