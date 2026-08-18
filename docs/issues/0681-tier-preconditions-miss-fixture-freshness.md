---
id: 681
title: "`check-tier-preconditions` probes the fixture STAMP but not fixture FRESHNESS, so it reports OK and `just ci` fails minutes later on the gate it did not run"
status: open
type: tech-debt
severity: medium
area: build, testing
related: [issue-0443, issue-0466, issue-0677, phase-318, rfc-0061]
---

## Observed

On a settled tree, immediately after a pull:

```
$ NROS_FIXTURE_LANE=native bash scripts/check-tier-preconditions.sh
preconditions exit=0            # no unmet precondition reported

$ just ci
...
ERROR: 10 compile-check fixture(s) are missing or stale:
  cpp_port_rclcpp_compat_smoke (stale build/cmake-fixtures/.../.inputsig)
  cpp_port_topic_state_monitor (stale …)
  cmake_add_subdir, cpp_robot_entry, cpp_port_minimal_publisher,
  pure_c_workspace, shadowing, c_mixed_workspace, local_msg_pkg, metadata_cpp
error: recipe `_check-fixtures-stale` failed with exit code 1
error: recipe `ci` failed
```

The preconditions pass, then the run dies on a precondition. That is the exact
failure `check-tier-preconditions` exists to prevent.

## Cause — two gates, two questions, one probed

`scripts/check-tier-preconditions.sh` runs five probes. The fixture one is:

```sh
probe "test fixtures missing or stale for this lane" \
    "just build-test-fixtures lane=${_fixture_build_lane}" \
    just _require-fixtures
```

`_require-fixtures` calls `nros_fixtures_stamp_require "$NROS_FIXTURE_LANE"` —
it reads the **stamp**, answering *"was a build run whose coverage includes
this lane's coordinates?"*

`_check-fixtures-stale` is a different gate answering a different question. It
reads `NROS_FIXTURE_SCOPE` (+ `NROS_FIXTURE_COORDS`) and audits **per-fixture
freshness**, comparing each `.inputsig` against its inputs — including the
`build/cmake-fixtures/` and `build/compile-check-fixtures/` families.

A stamp can be present and cover the lane while individual fixtures have gone
stale underneath it. The probe's own message says "missing **or stale**", but
the check behind it only establishes the first.

**`_check-fixtures-stale` is never probed.** The five probes are submodule
drift, CLI freshness, leaf `.cargo/config.toml` includes, `_require-fixtures`,
and workspace build-output residue.

## Why it matters more than one wasted run

[Issue 0466](archived/) built this script on a specific premise, stated in
CLAUDE.md: report every unmet precondition **at once** rather than one per
attempt. A gap in its coverage does not merely miss something — it converts a
cheap up-front report into an expensive discovery, and it does so while
answering "OK", which is worse than not being asked.

Cost here: `just ci` reached `test-all` — through `check`, `check-fast`,
`rust-rtos-link-check` and the whole gate tier — before failing. A rebuild plus
a full re-run.

[Issue 0443](archived/) already recorded that these two gates "reach the lane
under two different names" and fixed the two of them disagreeing about SCOPE vs
LANE. This is the same seam one step earlier: not the two gates disagreeing
with each other, but the PRECONDITION check knowing about only one of them.

Same family as [issue 0677](archived/) (the fixture build ran none of the
static gates that protect it): the gate is not missing, the EDGE to it is.

## Direction

1. **Probe both.** Add `_check-fixtures-stale` to
   `check-tier-preconditions.sh` beside `_require-fixtures`, with the same
   lane/scope wiring `just ci` uses — `ci` sets `NROS_FIXTURE_SCOPE` and
   `NROS_FIXTURE_LANE`, and #0443's fix means the script must set both or the
   staleness gate falls back to `all` and audits the tier-3 set.
2. **Or collapse them.** Two gates that a caller must remember to invoke in
   pairs are a seam that keeps producing issues (#0443, this one). If the
   stamp check and the freshness audit are always wanted together, one gate
   taking one scope is fewer things to get wrong.
3. Either way the probe's message should stop claiming more than it checks:
   today it says "missing or stale" while testing only the stamp.

## Not this issue

That the fixtures went stale at all is ordinary treadmill (the pull re-stamped
`nros-cpp`, which those cmake fixtures depend on) and is working as designed.
The defect is that the precondition check said OK about it.
