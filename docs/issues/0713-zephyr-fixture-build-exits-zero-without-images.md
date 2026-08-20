---
id: 713
title: "`just zephyr build-fixtures` configures ~70 build dirs, dies in its parallel layer, and exits 0 — so every Zephyr tier-2 coordinate fails MISSING"
status: open
type: bug
area: build/zephyr
related: [issue-0702, issue-0482, issue-0677]
---

## Symptom

Tier 2, with the zephyr fixture lane reporting success:

```
== zephyr == OK
```

then, in the test run:

```
Test fixture binary MISSING for an in-lane coordinate:
  .../zephyr-workspace/build-cortex-m-rust-talker-zenoh/zephyr/zephyr.elf
```

Seven of tier 2's thirteen failures are this, across
`zephyr_cortex_m_qemu` (cpp + rust), `qos_zephyr_ros2_interop_e2e`,
`logging_smoke`, `entry_matrix`, `multihost`, `realtime_tiers` and
`sched_dims_applied`.

## What is actually happening

The build dirs ARE created and configured — `CMakeCache.txt`, `build.ninja`,
`zephyr/arch`, `zephyr/drivers` are all present. What is missing is the LINK:
no `zephyr.elf`. Counted on this host after a full run:

```
$ ls zephyr-workspace/ | grep -c '^build-'      # 70
$ ls zephyr-workspace/*/zephyr/zephyr.elf | wc -l   # 9
```

and those 9 are leftovers from earlier one-off manual builds, not from the lane.

The run's own tail says why, and then exits 0 anyway:

```
make: *** wait: No child processes.  Stop.
make: *** Waiting for unfinished jobs....
make: *** wait: No child processes.  Stop.
[exited with code 0]
```

`wait: No child processes` is a make/jobserver fault, and this repo drives these
builds through a fifo jobserver (`NROS_JOBSERVER`, `just/zephyr-ci.just` around
the "fifo-jobserver token pool" and "fifo jobserver leaves omit `ninja -j`"
comments). NOT diagnosed further here — what is established is that the failure
is real, it is in the parallel layer, and it does not reach the exit status.

## Why it matters more than seven test failures

**A build lane that reports success having built nothing is the same defect
class as issue 0702**, one level up. 0702 was about tests that cannot fail;
this is a BUILD that cannot fail. Everything downstream inherits the lie: the
lane prints `== zephyr == OK`, `build-test-fixtures` exits 0, and the first
thing to notice is a test looking for an ELF twenty minutes later — where it
reads as a fixture-freshness problem rather than a build failure.

It also means **no Zephyr coordinate has been built by tier 2 on this host**,
and tier 2 is the only tier that builds Zephyr at all (tier 1 is native-only).

## Note on a wrong first reading

The first hypothesis here was that the lane SKIPPED zephyr on a stale stamp:
the fixture log shows `== zephyr ==` followed immediately by `== zephyr == OK`
with no build output between them, and only 8 build dirs existed at the time.
Running `just zephyr build-fixtures` directly then ran for >10 minutes and
produced 70 dirs, which looked like confirmation.

It was not. Re-running tier 2 afterwards cleared exactly ONE of the seven
failures. The dirs had been configured, not built — so the lane is not skipping,
it is failing silently, and the extra dirs changed nothing.

## Direction

1. Make the failure reach the exit status. Whatever `wait: No child processes`
   is, the lane must not print OK after it.
2. Then diagnose the jobserver interaction. The `NROS_JOBSERVER=1` path omits
   `ninja -j` / `CMAKE_BUILD_PARALLEL_LEVEL` deliberately, so a token-pool fault
   here starves the leaves rather than slowing them.
3. A post-condition worth having regardless: the lane knows which coordinates it
   was asked to build, and could assert their outputs exist before reporting OK
   — the same "assert the artifact" rule the fixture manifest applies elsewhere.
