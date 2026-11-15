---
id: 2606
title: "On a 4-core host the fixture build's static split gives each stage 1 job, so the last stage runs alone on 1 of 4 cores for hours"
status: open
type: perf
severity: medium
area: [build, testing, ci]
related: [0393]
found: 2026-11-15
---

**Found by** `just build-test-fixtures lane=tier1` in the ros2 distrobox,
on a host with `nproc` = 4 (box and host agree), while other sessions were
building on the same machine.

## Measured

The banner:

```
build-test-fixtures: budget=4, make-jobs=5, pool=4×1 + zephyr=4 (solo)
```

The stage joblog (`tmp/build-test-fixtures-*/build-test-fixtures.joblog`):

| stage | start (s) | duration (s) | jobs |
|---|---|---|---|
| zephyr | 0 | 1653 | 4 (solo) |
| threadx_linux | 1653 | 590 | 1 |
| linux | 1653 | 14150 | 1 |

For 13,560 s (about 3.8 h), linux was the only stage running and had
`NROS_BUILD_JOBS=1`, so 3 of 4 cores did nothing for this build. The
whole tier-1 build took 15,803 s.

## Why

The default (non-pooled) launcher in `justfile`'s `build-test-fixtures`
fixes `outer=4` and gives each stage `inner = budget / outer`. On a
4-core host that is 1. The split cannot give a stage more once the others
finish, which is the case the comment above the pooled launcher already
measured on 32 cores (a 25 % ceiling). On 4 cores the same rule is a 25 % floor for
the whole tail.

`NROS_BUILD_POOL=1` (one jobserver, stages draw from shared tokens) is
the opt-in that would have let linux take all 4 tokens after
threadx_linux finished. Not measured here yet.

## Small-host test-all overload (same run)

The tests step of `just ci tier1` on the same tree, at load average 12 on
4 cores: 14 failures, all timeouts.
- 4 `xrce_*` tests timed out at 240 s.
- `bridge_zenoh_to_cyclonedds` (2), `bridge_mixed_rmw::test_zenoh_to_xrce_bridge_e2e`.
- The `dds_isolation` (4) and `ros_env` (2) lib tests, and one `fixtures::cache_key` lib test, at 60 s.

Rerun alone with `cargo nextest run -j 1` on the same build: 17/17 and
4/4 passed, the slowest in 20 s. So these timeouts are load, not a
verdict on the code. A lane that runs on a small shared host needs
either a test-thread budget tied to `nproc` and current load, or a
timeout that scales with it.

## Proposed

- Make the pooled launcher the default, or at least pick it when
  `budget < outer * 2`.
- Re-measure this lane with `NROS_BUILD_POOL=1` on the same host.
- Decide how `test-all` sizes its test threads on a small host.

## Work on it

Phase-489 W13 (the make 4.4 token pool, branch `w13-token-pool`) makes the
pooled launcher the default when a pinned make 4.4 is present. Its 4-core
re-measure is the acceptance for this issue.
