---
id: 0357
title: "Tier 1's test filter excludes per-BINARY, so cross-platform cases inside generically-named binaries still run"
status: resolved
severity: P2
area: testing
created: 2026-07-31
resolved: 2026-07-31
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

## Resolution (2026-07-31)

`lane-filter.sh` now emits, in addition to the per-binary exclusions, ONE grouped
test-level expression:

```
(test(~tests::) or (not test(~esp32) and not test(~Esp32) and … ))
```

Three things that each had to be right:

- **Test-level, not just binary-level** — the original defect.
- **Both spellings.** nextest's `~` is a case-SENSITIVE substring match, and the
  harnesses disagree: rstest emits `platform_1_Platform__Freertos`, hand-rolled
  matrices emit `case_05_zephyr_rust`. One spelling covers half the suite.
- **A unit-test exemption.** Without `test(~tests::)` the lane also dropped
  host-only tests that merely *mention* a platform —
  `board::tier::tests::threadx_inverts_scale`, `qemu::tests::test_parse_results`,
  and `zephyr::tests::content_aware_staleness_ignores_mtime_only_bumps`, a
  phase-318 test. Those need no fixture and no toolchain; excluding them trades
  one coverage hole for another. Matched without leading colons so it also catches
  top-level `tests::…` and `applicability_tests::…`.

### Measured (`cargo nextest list`, so it measures SELECTION, not a pass/fail run)

| selection | tests |
| --- | --- |
| unfiltered | 1479 |
| binary-only (before) | 1360 |
| binary + test (after) | 1263 |

97 newly excluded, **all 97 name a non-native platform**, and **all 53 of the
cross-platform failures from the acceptance run are now deselected (0 survivors)**.
592 unit tests remain selected.

### A measurement trap worth recording

The first version of this measurement passed each filter line as a separate
nextest `-E` flag and reported that NOTHING was ever excluded — suggesting the
filter had never worked at all. That was the harness, not the product: **nextest
UNIONs multiple `-E` flags**, and `not A or not B` is a tautology. `just test-all`
joins with `" and "` into a single `-E` (justfile ~1385), which is correct. A
measurement harness that does not match the production composition measures
itself. `lane_filter_test_exclusions_are_one_grouped_conjunction` now pins the
shape so a future edit cannot emit something that only composes under OR.

### The gate that let this through

`lane_filter_tokens_cover_every_non_native_platform` asserted the TOKEN list was
complete and would have passed forever. It now also asserts a `test(~family)` and
a `test(~Family)` exclusion per platform, plus the exemption. Both new gates were
negative-tested: reverting the fix makes them FAIL, which the original never could.

### Residue — deliberately not fixed here

Name-based filtering cannot distinguish "needs a cross-platform fixture" from
"merely mentions a platform". ~5 host-only tests are still excluded
(`cmake_platform_threadx_requires_board`,
`example_shape zephyr_leaf_buildrs_uses_shared_bake`,
`kconfig_platform_default_drift zephyr_kconfig_mirrors_platform_toml_tx_defaults`,
`platform test_zephyr_{environment,workspace}_detection`). The rest of the residue
genuinely needs a target or toolchain (FVP west build, esp-idf, platformio, QEMU
runs) and several are already toolchain-gated separately.

The principled fix remains option (2) above: have the matrix consumers read the
lane and `skip!` an out-of-lane cell, so the decision is made where the cell is
known instead of inferred from a name. That is a larger change across many test
files and did not need to block this one.
