---
id: 854
title: "`action_raw_goal_ships_one_cdr_header` times out in-sweep and passes solo
  with a 16x margin — starved, not slow"
status: open
type: bug
area: testing
related: [phase-395]
---

## Problem

`action_raw_goal_ships_one_cdr_header` hits its 60 s nextest timeout when it runs
as part of a full sweep, and completes in **3.6 s** when run alone — 5/5 solo
passes.

It is the first entry in `.config/flake-quarantine.toml` (phase-395 W5), so it
still runs and still records, but no longer blocks.

## Why this is starvation rather than a slow test

The assertion is that a raw goal ships exactly one CDR header. That property does
not depend on timing. What depends on timing is *reaching* the assertion inside
the budget while ~31 other gates compete for the host.

A test that needs 3.6 s and is given 60 s is not close to its budget. A 16×
margin does not erode into a timeout because the code got slower; it erodes
because the process did not get scheduled. That is the distinction the quarantine
evidence rule exists to force: "it failed once" is indistinguishable from a real
intermittent defect, and quarantining a real defect is how one ships.

## What has NOT been established

- **Which resource is contended.** CPU is the obvious guess and is not evidence.
  The sweep also contends on the loopback network, on the router, and on the
  fixture target dir. Nothing here distinguishes them.
- **Whether it is the only test in this shape.** A 16× margin collapsing under
  load suggests the budget model is wrong for a whole class, not for one test.
  `nextest-slow-tests.py` reports durations; nobody has compared in-sweep against
  solo across the suite, which is the measurement that would answer it.

## The in-sweep vs solo comparison, done — for the UNIT lane

"Whether it is the only test in this shape" was one of the two things this issue
said had not been established. It is now answered for the unit suite, and the
answer is no class effect.

First, the suite this measures is not the one it was written against.
[[issue-0959]] found that a session teardown waited out a whole keep-alive
interval — 20 s after #0906 raised the lease — and fixing it took the unit suite
from **129 s summed to 17.4 s**. Six tests had been 20.2 s each, 94 % of the
total. Any budget reasoning from before that is reasoning about a different
suite.

After it, `just test-unit` over 1037 tests leaves only SIX at or above 0.5 s.
Each run solo:

    test                                          in-sweep   solo    ratio
    test_pubsub_loopback_with_scouting_disabled     4.16 s   4.154 s   1.00
    test_pubsub_loopback                            4.16 s   4.154 s   1.00
    two_sessions_deliver_cross_session_through_r    1.15 s   1.154 s   1.00
    test_multiple_subscribers                       1.15 s   1.155 s   1.00
    zenoh_event_matrix                              1.10 s   1.104 s   1.00
    test_multiple_publishers                        1.15 s   0.154 s   7.5x  (*)

(*) Not contention. Run solo three times it gives 0.154 / 1.154 / 1.154 — it
varies by exactly 1.0 s IN ISOLATION, so the in-sweep figure is simply its common
case. The test has no sleeps at all (open a session, create two publishers,
close), so the step is in router startup or session open; `wait_for_router_ready`
polls at 100 ms, which cannot produce a clean 1.0 s step, so something else
retries on a one-second boundary. Small, real, and not this issue.

**Conclusion for the unit lane: nothing is starved.** Five of six are identical
to three significant figures, and the sixth is bimodal on its own. There is no
class of tests whose budget model collapses under load here — after #0959 the
worst unit test is 4.16 s against budgets measured in tens of seconds.

## What this does NOT settle

`action_raw_goal_ships_one_cdr_header` is FIXTURE-BACKED
(`packages/testing/nros-tests/tests/action_raw_goal_e2e.rs`), so it is not in
the unit suite and none of the above measures it. Its lane needs a `lane=all`
fixture build to compare, which this work did not run.

So the honest position is narrower than "0854 is explained":

* the "is it a whole class?" question is answered NO for the unit lane;
* it is unanswered for the fixture lane, which is where this test lives;
* and #0959 removed 20 s from every session teardown, which plausibly changes
  this test's in-sweep behaviour too — it opens a session. Plausibly is not
  measured. Re-run it in a full sweep before deciding whether the quarantine
  entry can go.

## Why the quarantine expires 2026-10-27

Quarantine without expiry is deletion with extra steps. Two months is long enough
that the answer can come from the contention measurement above rather than from
guessing, and short enough that the entry cannot be forgotten.

## Not to do

Do not raise the timeout to make it green. That converts a diagnosable
starvation signal into a longer sweep that still fails under heavier load, and it
destroys the 16× margin that is currently the evidence for the diagnosis.
