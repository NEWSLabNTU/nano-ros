---
id: 804
title: "31 tests turned \"this host has no ROS router\" into a failure, so tier 2
  could not go green on any machine without a ROS install"
status: resolved
type: bug
area: testing
related: [issue-0599, issue-0695, issue-0774, phase-362]
---

## Problem

`just ci-matrix` reported **7 real failures** on this host. All seven were the
same thing, and none was a defect in the code under test:

```
nros-tests::emulator  test_qemu_bsp_pubsub_e2e
nros-tests::emulator  test_qemu_rtic_{pubsub,service,action,mixed_priority}_e2e
nros-tests::emulator  test_qemu_serial_pubsub_e2e
nros-tests::large_msg test_qemu_zenoh_large_publish
```

each failing with

```
Failed to start zenohd with serial listeners: RouterUnavailable(
  "no `rmw_zenoh_cpp/rmw_zenohd` found ... AMENT_PREFIX_PATH=unset, ROS_DISTRO=unset")
```

Retested solo, per the QEMU-flake rule: identical, so not load. The host simply
has no ROS — it lives in the `ros2` distrobox, and CLAUDE.md forbids mixing that
into the host tree, so "install ROS" is not the remedy.

`ZenohRouter::start*` returns a `TestResult` whose `RouterUnavailable` variant
means exactly "not runnable here", and `fixtures::or_skip` is the one place that
reading lives — it raises a capability skip for that variant and leaves every
other error a hard failure, because a router that IS present and refuses to
start is a real fault. Written for issue 0599, whose own doc comment says "a
lane that cannot run must say so".

**31 call sites went around it with `.expect(...)`.** `or_skip` was reachable
only through the `zenohd()` / `zenohd_unique()` rstest fixtures; no direct caller
used it. Same shape as the vtable slots in issue 0800: the right mechanism
existed, was documented, and nothing was wired to it.

## A second defect, found on the way, in the same failure

Before those seven were even reached, `test-zpico-multisession` failed with

```
ERROR: nextest build/setup failed (nextest exit 100) — not a [SKIPPED] precondition.
```

Its two `loan_e2e` tests were already skipping correctly. What went wrong is the
tolerance layer. Issue 0695 made `nros_nextest_junit_path` derive the junit from
`CARGO_TARGET_DIR`:

```sh
printf '%s/nextest/%s/junit.xml\n' "${CARGO_TARGET_DIR:-target}" "$profile"
```

That is false on cargo-nextest 0.9.143. With `CARGO_TARGET_DIR` exported, the
BUILD honours it — binaries land in `target-zpico-multisession/<profile>/deps/`
— and the junit still goes to `target/nextest/<profile>/junit.xml`. So the
derived path named a file that never appeared, `_nextest-tolerant` took its
"no junit means the build failed" branch (correct in itself: a target that fails
to compile emits zero cases and would otherwise tally as zero real failures),
and every `[SKIPPED]` in that lane became a hard red.

0695's concern was real — a reader must not tally whatever unrelated run last
wrote the default path — so the fix keeps it: list both candidates, delete them
before the run, and read back whichever nextest actually wrote. Staleness is
answered by the delete rather than by predicting the path.

## Fix

- `fixtures::or_skip` at all 31 sites, across 12 test files. Mechanical: the
  transform only moves `RouterUnavailable` from failure to skip; every other
  error still panics.
- The doc example on `ZenohRouter` taught `.unwrap()`. It now teaches `or_skip`.
- Gate `just check-zenohd-router-skips` (fast line): no `.expect(` / `.unwrap()`
  directly on a `ZenohRouter::start*` call, balanced-paren aware so a nested
  call or a `)` inside a string does not end the scan early. It deliberately
  does NOT flag `unwrap_or_else(|e| skip!(...))` — that is over-tolerant rather
  than under-tolerant, a different argument.
- `nros_nextest_junit_reset` + resolve-after-run in `_nextest-tolerant`, `test`
  and `test-all`; `test-failed` is fixed by the resolve-on-call change alone.

## Result

`just ci-matrix` green: 1695 tests, 1367 passed, **0 real failures**, 328
rewritten to skips (capability 270, lane 58).

Worth stating plainly rather than reading that as a full sweep: 270 capability
skips means a large part of tier 2 did not execute on this host, because there
is no ROS router and no emulator for several lanes. What the run establishes is
that nothing among the 1367 that COULD run fails — and, now, that the ones which
cannot run say so instead of failing.
