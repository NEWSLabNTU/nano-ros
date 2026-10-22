---
id: 1991
title: "`tier 2 nightly (pairwise cover)` uploads no junit and prints no failure output, so 11 of its 14 real failures cannot be read"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-20
related: [1988]
---

## Measured

- Run: nightly 39373548195 (schedule, 2026-10-20 05:11 UTC, main at
  `d2adb51f7`), runner `nano-ros-runner-newslab-server-241`.
- Job: 118466509430, `tier 2 nightly (pairwise cover)`, step
  `just ci matrix-nightly`. The fixture build in the same job passed:
  `All test fixtures built.`

```
     Summary [ 275.026s] 3122 tests run: 2893 passed (4 slow, 1 flaky), 225 failed, 4 timed out, 7 skipped
rewrite-skipped-junit: skips by class: lane=215
Real failures: 14 / 14 total failures
```

The run has one artifact, `zephyr-gate-inputs`, and no junit. `test-all` runs
nextest with `--failure-output ${NROS_TEST_FAILURE_OUTPUT:-never}`, so the log
holds no panic text either. The 14 real failures are counted and not named.
The job in `.github/workflows/nightly.yml` has no upload step between
`just ci matrix-nightly` and `Sweep orphans and disk`. The tier-1 and tier-2
jobs in `run-matrix.yml` have one (`Upload junit and logs`, `if: always()`),
and so do the per-platform nightly jobs. The junit stays in the persistent
fixture tree on the runner, where the next nextest run there rewrites it.

## Which 14, and how they were found

The log does not name them, so they were found by elimination.

1. The summary lists 229 red tests. A test counts as a skip only if its panic
   carries `[SKIPPED…]` (`scripts/test/rewrite-skipped-junit.py`). All 215
   skips here were `lane` class.
2. All 229 were re-run at `d2adb51f7` in a scratch worktree with no
   fixtures, under the job's own environment:
   `NROS_FIXTURE_LANE=tier2-nightly`, `NROS_TEST_COORDS` from
   `nros_lane_coords_file tier2-nightly`,
   `NROS_TEST_UNCLAIMED=ros2,host-bridge`. 196 lane-skipped. 33 did not.
3. Dummy executables were placed at the missing fixture paths and the 33 were
   run again. 16 more lane-skipped, at an out-of-lane second fixture or at
   `require_ros2_claimed`.
4. Three more were assigned to skips by reading the code, not by running it:
   - `linux_api test_c_rust_pubsub_interop` and `test_cpp_rust_pubsub_interop`
     resolve the C/C++ talker only after the Rust listener is ready.
     `linux,c,zenoh` and `linux,cpp,zenoh` are not in the lane, and the
     sibling `*_service_interop` tests lane-skip on exactly those coordinates.
   - `multihost_partition_bake multihost_bake_emits_only_the_hosts_node`
     censuses the in-lane Rust cells and then resolves `c robot1`, whose
     coordinate (`linux,c,zenoh`) is out of lane. Its CI time of 2.55 s fits
     two censuses and then a skip.

That leaves 14. The CI durations of the three marked "known" below match the
tier-2 sighting in issue 1988. Their text is not available here.

| # | test | CI result |
| --- | --- | --- |
| 1 | `nano2nano test_gid_consistency` | FAIL 20.6 s (issue 1988) |
| 2 | `nano2nano test_sequence_number_increment` | FAIL 21.0 s (issue 1988) |
| 3 | `qos test_qos_keyexpr_encoding` | FAIL 1.9 s (issue 1988) |
| 4 | `condvar_contract_rtos condvar_contract_holds_on_threadx_linux` | TIMEOUT 60 s |
| 5 | `logging_smoke logging_smoke_threadx_linux_captures_stderr` | TIMEOUT 60 s |
| 6 | `pool_exhaustion_threadx_linux zenoh_pool_exhaustion_reports_full_and_says_why_on_firmware` | TIMEOUT 60 s |
| 7 | `entry_build entry_demos_build::case_2_threadx_linux` | TIMEOUT 60 s |
| 8 | `rtos_e2e test_rtos_pubsub_e2e::…Nuttx::lang_1_Lang__Rust` | FAIL 0.18 s |
| 9 | `rtos_e2e test_rtos_service_e2e::…Nuttx::lang_1_Lang__Rust` | FAIL 0.28 s |
| 10 | `rtos_e2e test_rtos_action_e2e::…Nuttx::lang_1_Lang__Rust` | FAIL 2.4 s |
| 11 | `param_store_carve nuttx_arm_carves_the_parameter_store_and_delivers` | FAIL (retried: 0.94 s, 0.18 s) |
| 12 | `entry_build entry_demos_build::case_1_nuttx` | FAIL 1.2 s |
| 13 | `emulator test_qemu_wcet_benchmark` | FAIL ×3 (1.39 s, 0.61 s, 0.41 s) |
| 14 | `entry_e2e entry_matrix` | FAIL 100.7 s |

### Sighting, 2026-10-22

- Run: nightly 39726654020 (schedule, 05:12 UTC, main at `3feb834d1`).
- Job: 119651990002, `tier 2 nightly (pairwise cover)`, on
  `nano-ros-runner-newslab-118-server`.

The run got past the build this time and still left its failures unread:

```
     Summary [ 198.758s] 3168 tests run: 2941 passed, 227 failed, 7 skipped
Real failures: 12 / 12 total failures
```

There is still no junit artifact. The 12 are counted and not named.

## Reading

What the durations and neighbours show, and nothing more:

- **Rows 4–7: four threadx-linux images, each killed at nextest's default
  60 s.** Other threadx-linux tests in the same run passed, and those have
  longer budgets: `rtos_e2e` pubsub took 162 s, service 84 s, action
  96–101 s, and `linux_api test_threadx_linux_cyclonedds_action` took 27 s.
  The log cannot say whether these four images hung or were only slow.
- **Rows 8–12: every NuttX-ARM Rust image in the run.** The C and C++ cells
  of the same `rtos_e2e` tests passed (pubsub 70 s, service 0.6 s, action
  3.5–4.4 s). Rows 8, 9 and 11 fail in under 0.3 s. That is resolver or boot
  time, not a delivery timeout, but the log does not show which.
- **Row 13:** retried three times, failing faster each time.
- **Row 14:** `entry_matrix` iterates 18 cells in one test, and the log does
  not say which cells failed.

## What it is NOT

- Not a fixture build failure. The job's build step printed
  `All test fixtures built.` for all seven families.
- Not the 215 skips. Every one was `[SKIPPED:lane]`.
- Not issue 1989 (`interface_subset_linkage`). Both of its tests lane-skipped
  here.

## What would close it

1. **Make the lane readable.** Add the `Upload junit and logs` step
   (`if: always()`, the `${{ env.NROS_FIXTURE_TREE }}/target/nextest/**/junit*.xml`
   glob `run-matrix.yml` uses) to the `tier 2 nightly (pairwise cover)` job.
   Or set `NROS_TEST_FAILURE_OUTPUT=final` for its run step. Either one is
   enough.
2. **Then read the next red night's text** for rows 4–14 and file each
   distinct cause, or attribute it to an existing issue. Until then, rows 4–14
   are a list of names, not a diagnosis.
