---
id: 681
title: "`check-tier-preconditions` probes the fixture STAMP but not fixture FRESHNESS, so it reports OK and `just ci` fails minutes later on the gate it did not run"
status: resolved
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


## RESOLVED 2026-08-19 — probe both, and stop the message claiming more than it checks

Took direction 1 + 3. `check-tier-preconditions.sh` now runs `_check-fixtures-stale`
beside `_require-fixtures`, and the two probes say which question each answers:

    [x] test fixtures MISSING for this lane (no build stamp covering it)
    [x] test fixtures STALE for this lane (an input is newer than the artifact)

No scope wiring was needed: issue 0443 already made `_check-fixtures-stale`
DERIVE its scope from `NROS_FIXTURE_LANE`, which this script reads, so the two
gates cannot disagree about what the lane contains.

### Demonstrated on the tree that reported the bug

Same checkout, immediately after a pull:

| gate | exit |
| --- | --- |
| `_require-fixtures` (the only one probed before) | **0** — the batch reported OK |
| `_check-fixtures-stale` (never probed) | **1** — 54 stale C/C++ cells, then 9 more families |

With the probe added, the batch now reports the unmet precondition up front
instead of `just ci` dying on it after `check`, `check-fast` and
`rust-rtos-link-check`.

### One property this breaks, deliberately

The probes around it are documented "buildless and source-free". This one is
not: `_check-fixtures-stale` self-heals the C/C++ cmake cells it finds stale
("54 cell(s) … have now been rebuilt"), so the batch can now do work rather than
only report. It is the same work `just ci` would do minutes later, moved
earlier — but the difference from its neighbours is real, and the script says so
rather than leaving the next reader to discover it.

### Direction 2 — DONE 2026-08-19, in a follow-up commit

Collapsed after all. `_require-fixtures-ready` is now the one name for "the
fixtures this lane needs are ready", and it is what `test`, `test-all` and the
precondition batch call. The two halves stay as private implementation:

    _require-fixtures-ready: _require-fixtures _check-fixtures-stale

Order is load-bearing and stated in place: stamp FIRST, because with no build at
all the freshness audit has nothing to compare and its message would describe
the wrong problem. Both halves already derived their scope from
`NROS_FIXTURE_LANE` (#0443), so the collapsed gate takes ONE scope and cannot
disagree with itself.

Every caller invoked them as a pair already — `test`, `test-all`, and this
batch after the fix above — which is what made the collapse safe rather than a
behaviour change. What it removes is the possibility of a FUTURE caller naming
one and not the other, which is the seam that produced #0443 and this issue.

The batch is one probe again, since one remedy answers both:

    [x] test fixtures not ready for this lane (missing, or stale under the stamp)

Verified both arms still fire: on a tree with a `native` stamp and stale cells,
`NROS_FIXTURE_LANE=native` reports the freshness failure; `NROS_FIXTURE_LANE=all`
reports "fixtures were built for lane 'native', but this run needs ALL of them"
— the stamp arm, first, as the ordering intends.

**One thing worth knowing for the next justfile edit:** `just --evaluate`
reported the file as parsing while it was broken. Inserting a comment block
between an existing `[private]` and its recipe orphaned the attribute
("extraneous attribute") and silently stripped `_check-fixtures-stale`'s
privacy; only `just --list` (or running a recipe) surfaced it. Use one of those
to check a justfile edit, not `--evaluate`.
