---
id: 2742
title: "`entity_wake`'s kilohertz-arrival test fails its own control on a busy host: an absolute 300 arrivals/s the publisher cannot reach under load"
status: resolved
type: bug
severity: low
area: [testing, gates, executor]
related: [issue-2683]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "test(entity-wake): the kilohertz control counts arrivals instead of requiring a rate"
---

## Measured

`nros-tests::entity_wake an_idle_member_parks_through_kilohertz_arrivals_for_another`
(`packages/testing/nros-tests/tests/entity_wake.rs`) failed in a full
`just ci gate` on host 175, at `check::build` (via
`build::check::required-features-tests`). The PR under test (#4369) changed
only parameter-descriptor truncation.

The failure was the test's CONTROL, not its property:

```
phase-397 W6: idle member over 3.0015527s: 6 spins (2.0/s), 114us thread CPU
(0.004 % of a core), while another member took 249 arrivals/s
panicked at packages/testing/nros-tests/tests/entity_wake.rs:975:5:
only 249 arrivals/s were published, too few to tell a fan-out member from a parked one
```

The idle member parked as it should: 6 spins in 3 s, 114 us of thread CPU.
The publisher thread reached only 249 arrivals/s against the control's
absolute floor of 300. The host's 1-minute load was about 30-50 during the
gate, with two unrelated builds running.

The same test run alone on the same host and worktree, at a load of about 25-33,
passed three times out of three:

| run | idle spins | idle CPU | arrivals/s |
|---|---|---|---|
| 1 | 6 (2.0/s) | 110 us | 796 |
| 2 | 6 (2.0/s) | 101 us | 775 |
| 3 | 6 (2.0/s) | 106 us | 803 |

## Cause

The control asserts `rate >= 300.0`: an absolute publish rate that depends
on a publisher thread getting CPU, which a loaded host does not guarantee.
When the host cannot meet it, the test fails, although what it measures
(an idle member does not wake for another member's arrivals) held. This is
the load sensitivity issue 2683 describes for `check::build`, in one test's
precondition.

## Direction

Make the control relative to the run, not absolute: judge the idle member's
wakes against the arrivals actually published in that run. For example,
idle wakes stay at most k times its timer-driven spins while the arrivals
are at least N, so a fan-out member would have woken N times. When the host
cannot produce enough arrivals to tell the two apart, report the test as a
skip that names the shortfall. A precondition the host cannot meet is not a
failure of the property.

Owner: executor tests (nros 142), routed by the manager.

## Resolution

The control is a COUNT of arrivals, not a rate. A fan-out member spins once
per arrival, so the verdict needs enough arrivals that one would spin far
past the park bound:
- `MIN_ARRIVALS` is ten times the park bound at the longest run (320).
- The run lasts 3 s, and continues up to a 15 s cap until it has seen that
  many arrivals.
- The spin bound is computed over the time the run actually took.

At 249 arrivals/s the run sees about 750 arrivals in 3 s, so the gate's case
passes on its merits.

The skip in "Direction" was not taken. CLAUDE.md's rule is that a missing
precondition is a red and only a lane's scope may skip, and
`check-skip-budget` fails any other skip. A host that cannot make 320
publishes in 15 s still fails, naming the shortfall.

Measured:
- solo: 912 arrivals/s, 6 spins in 3.0 s; pass.
- Under 2 x nproc spinning shells: 935 arrivals/s; pass. The 249/s did not
  reproduce on this host.
- `MIN_ARRIVALS` forced to 4000 to exercise the stretch: the run went to
  4.5 s, with 9 spins against a bound of 11; pass.
