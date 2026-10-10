---
id: 1788
title: "On the new self-hosted runner, `AMENT_PREFIX_PATH` is set but indexes no packages, so `cli-tests`' `with_ros` cases fail with \"No ROS packages found\""
status: open
type: bug
area: [ci, runner]
severity: medium
found: 2026-10-10
related: [1365, 1779, 1764]
---

## Measured

`run-matrix` run **37945434581**, dispatched on `main` at `2b8153621` on
2026-10-09 at 14:36Z. It waited about 14 h for a runner (issue 1365). Job
**114110273467**, `tier 2 (1-wise matrix)`, then ran on the newly registered
**`nano-ros-runner-newslab-133`** (started 04:39:56Z). The step `just ci matrix`
failed at its first stage:

```
check-build (parallel): 3 of 25 gate(s) FAILED
<== ci tier2 [1/4] check::default — FAILED after 14m20s (at 05:29:29Z)
```

This issue is about one of the three red gates, `cli-tests`:

```
with_ros::test_discover_ament_packages --- FAILED
with_ros::test_ament_index_available --- FAILED
with_ros::test_find_std_msgs --- FAILED
Found 0 ROS packages
thread 'with_ros::test_discover_ament_packages' panicked at cargo-nano-ros/tests/integration_tests.rs:200:9:
No ROS packages found
```

`is_ros_available()` (`integration_tests.rs:18`) only asks whether
`AMENT_PREFIX_PATH` is SET. So the job environment had the variable set, but
`AmentIndex::from_env()` found zero packages under it. Either the value is
empty, or it names prefixes that hold no `share/ament_index` inside the runner
container.

## What it is NOT

- **Not the `AMENT_PREFIX_PATH` race.** The tests already hold
  `AMENT_ENV_LOCK` (`803d512fe`), and that commit is an ancestor of
  `2b8153621`.
- **Not #1781's precedence change** (`fbccb5e58`, first prefix wins). That
  changes which package wins when two prefixes both have it. It cannot turn a
  non-empty index into an empty one.
- **Not the other two red gates in the same run.** Both are fixed on main
  after `2b8153621`:
  - `workspace-features` failed on `rust-lld: error: duplicate symbol:
    nros_platform_clock_resolution_ns` (and four more `nros_platform_*`
    symbols). That is issue 1779, fixed by #1864.
  - `template-copy-out` failed with "this workspace declares no `[image.*]`".
    That is #1764, fixed in `1b998382e`.

## Not established

- **Whether this runner or the image is at fault.** This is the first tier-2
  verdict from `newslab-133`. The previous registration, `newslab-118`, ran
  no job at all after 2026-10-09T08:00Z (issue 1365), so there is no recent
  run on the same code to compare against.
- **The variable's actual value.** The job log does not print it. The test
  prints it only on success (`test_ament_index_available`).

## What would close it

1. Print `AMENT_PREFIX_PATH`, and whether each of its entries has
   `share/ament_index/resource_index/packages`, inside the runner container
   (`runner-doctor.sh`, or a one-off step).
2. If the value names a ROS install the image does not carry, fix the image
   (CLAUDE.md: "a missing dependency is an IMAGE fix"), or stop exporting the
   variable where ROS is absent.
3. Then a tier-2 run on current `main` shows `cli-tests` green on this runner.
