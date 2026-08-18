---
id: 673
title: "`check-required-features-tests` runs bare nextest, so a capability SKIP is a tier-1 red on any host without the ROS zenoh router"
status: resolved
type: bug
area: ci/testing
related: [phase-362, phase-366]
---

## Symptom

`just ci` (tier 1) fails at `check-required-features-tests` on a host with no
`ros-<distro>-rmw-zenoh-cpp` installed:

```
Summary [   0.994s] 20 tests run: 7 passed, 13 failed, 0 skipped
  FAIL nros-tests::component_runtime runtime_registers_single_component_and_spins_once
  ... 12 more
error: recipe `check-required-features-tests` failed with exit code 100
```

All thirteen "failures" are one message, from one line:

```
thread '...' panicked at packages/testing/nros-tests/src/fixtures/zenohd_router.rs:448:13:
[SKIPPED:capability] no `rmw_zenoh_cpp/rmw_zenohd` under /opt/ros (ROS_DISTRO=humble).
```

Zero other panics in the run.

## Cause

Two correct decisions that do not compose.

Phase-362 W3/W5 made the zenoh lanes run the router **a ROS 2 deployment
actually runs**, and made a host without it SKIP rather than silently substitute
`zenohd`. The skip is spelled `nros_tests::skip!`, which is a panic carrying a
`[SKIPPED:capability]` marker — the only mechanism available, since Rust's test
harness has no runtime skip.

`check-required-features-tests` (justfile ~654) invokes `cargo nextest run`
directly. Only `just test-all` post-processes the junit output to turn those
marked panics back into skips. So in this one lane the marker is never read and
every capability skip is a hard failure.

This is the pitfall CLAUDE.md already names — "Bare `cargo nextest` counts
`nros_tests::skip!` panics as FAILURES" — reached from the other direction: not
a human running nextest by hand, but a `just ci` step doing it.

## Why it matters

- **Tier 1 is the default lane, and it is red for an environment fact.** The
  instruction is to run the tier your change earns after every task; a lane that
  cannot go green without an apt package teaches people to read its red as
  noise, which is how a real red gets skipped past.
- **It hides everything after it.** CI stops at the first failing step, so
  `check-feature-set-ssot`, `check-no-tracked-file-find`, `native::check`,
  `rust-rtos-link-check` and `test-all` never run. Discovered during phase-366:
  the step ran for the first time in that session only after an earlier gate was
  fixed, and it then masked the remaining two thirds of tier 1.
- **The lane exists to prove `required-features` targets are reachable** (issues
  0652/0612/0667). Its own reds being unreadable defeats the point of laning
  them.

## Directions

Candidates, not a plan.

- **Give this lane the junit rewrite `test-all` already has.** One mechanism,
  used everywhere nextest runs under `just`. The rewrite currently lives inside
  `test-all`; factoring it into a shared helper is the actual work, and is the
  same "one shared helper, not a second spelling" rule the repo applies
  elsewhere.
- **Make the capability probe a lane precondition** rather than a per-test skip,
  so the lane reports "cannot run here" once instead of thirteen times. Loses
  the ability to run the seven tests that do not need the router.
- **Ship the router.** `NROS_RMW_ZENOHD` can point at a built binary; a
  provisioning step could produce one the way `build/zenohd/zenohd` is produced.
  Costs a build and re-opens the version-pairing question phase-362 closed.

Whichever way: the invariant worth keeping is that **`nros_tests::skip!` means
the same thing in every lane that runs tests.** Today it means "skip" in one and
"fail" in another.


## RESOLVED 2026-08-18 — one interpreter for the marker

Took the first direction: give this lane the junit rewrite, by FACTORING it out
rather than copying it. `_nextest-tolerant +nextest_args` is now the single place
`[SKIPPED]` is interpreted; `_nextest-platform` delegates to it (it had carried
its own copy) and `check-required-features-tests` calls it instead of running
`cargo nextest` bare.

It takes the nextest arguments verbatim, so callers keep their own `--features` /
`--test` spelling — which matters because `check-required-features-reachable`
reads reachability off the literal `--features` text in the justfile, and hiding
it behind a variable would make that gate narrower than its rule (issue 0196).

### Verified, both directions

This host HAS `ros-humble-rmw-zenoh-cpp`, so a green run proves nothing about
the case the issue is about. Simulating a router-less host with
`NROS_RMW_ZENOHD=/nonexistent/rmw_zenohd`:

| | result |
| --- | --- |
| fixed lane, no router | `20 tests run: 7 passed, 13 failed` -> **exit 0**, "All failures were [SKIPPED] preconditions" |
| the OLD bare `cargo nextest` form, same env | **exit 100** — the reported red |
| a setup failure (`--test no_such_test_target_xyz`) | **exit 1**, "build/setup failed (nextest exit 101) — not a [SKIPPED] precondition" |
| the lane on this host, router present | `20 tests run: 20 passed` |

The third row is the one that matters for trusting the other two: the tolerance
is keyed on the MARKER, and a build/setup failure (nextest exit != 100, or no
junit) is still a hard red. `_check-skip-budget` runs on the success path too, so
"all failures were skips" cannot quietly become "nothing ran".

Existing `_nextest-platform` callers are unaffected — 7 call sites, and
`custom_transport_loopback` (the lane added for issue 0652 the same day) still
passes through the delegation.

### The invariant this restores

`nros_tests::skip!` now means the same thing in every lane that runs tests. It
previously meant "skip" under `test-all` and `_nextest-platform`, and "fail"
under this one — which is the CLAUDE.md pitfall "bare `cargo nextest` counts
`skip!` panics as FAILURES", reached not by a human running nextest by hand but
by a `just ci` step doing it.
