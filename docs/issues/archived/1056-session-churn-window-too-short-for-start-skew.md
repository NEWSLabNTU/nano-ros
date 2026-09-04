---
id: 1056
title: "`assert_no_session_churn` can PASS on a broken lease, because start skew
  can push one node's only lapse past the 60 s window"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-09-04
resolved: 2026-09-05
related: [issue-1044, issue-1013, issue-0906]
---

## The hole

`MAX_ROUTER_SESSIONS = 3` stands in for a rate with a count, and issue 1044
recorded the arithmetic in its doc rather than closing it. A client lease
`L < 30 s` cannot hear the router's 30 s keep-alive, so each node re-dials every
`2L`. In a 60 s window that is one re-open per node, two nodes, four sessions —
over the limit, correctly.

That holds only if BOTH nodes lapse inside the window. They do not start
together: the listener is launched after the talker's readiness banner. For a
lease near the top of the band (say 29 s, lapsing at 58 s) the later node's only
lapse can land past the window's end, leaving **3 sessions and a PASS on a build
that is broken**.

The exposure is to start SKEW, not to the lease value, which is why it is not
visible in the lease table: every lease in the band is "covered" on paper.

## Why the obvious fix is unavailable

Counting per NODE would make skew irrelevant — the slack becomes "one re-open
each" and one node re-dialling twice is caught however the other behaves. It
cannot be done from the router log: **the client zid is regenerated on every
session open.** `zpico.c`'s `zpico_next_session_zid_counter()` mixes a monotonic
counter and the clock into the zid, so two opens by one node carry two different
identities and grouping by zid reads as more NODES rather than more sessions.
(Measured by reading the generator, not inferred from the log.)

## Direction

**Lengthen the window to ~120 s.** At that length every lease under 30 s produces
at least two re-opens per node, so no amount of skew can hide one, and the count
becomes sufficient again. The cost is real and is the whole reason it was not
just done: `PUBSUB_MIN_SAMPLES` is 60 at 1 Hz, so this roughly doubles the
pub/sub cell — currently ~61 s per language per platform.

Worth pricing against the alternative before spending it: a delivery assertion
cannot substitute (issue 1013 measured a 10 s lease delivering 60/60, because the
reopen now completes in ~15 ms), so the session count is the only signal there
is, and a signal that can be skewed into silence is the thing being bought back.

## Not a regression

Nothing here is newly broken — this is the standing accuracy of a check that has
always been a count. It is filed because issue 1044 established the bound
precisely enough to act on, and a test that can pass on a build it exists to
reject should not live only in a doc comment.

## Acceptance

A build with `Z_TRANSPORT_LEASE_MS` anywhere in `(0, 30_000)` fails this cell
regardless of how far apart the two nodes start.


## RESOLVED 2026-09-05 — and the mechanism above is WRONG in two places

Fixed by `PUBSUB_MIN_SAMPLES` 60 -> 70, with each `pubsub_window` arm raised
+10 s to keep its headroom. But the reasoning that produced the 120 s proposal
does not survive reading the cell, and the corrections are the useful part.

**1. The listener is spawned FIRST, not after the talker.** `rtos_e2e.rs` starts
the listener, sleeps `stabilization_delay()` — 20 s on freertos / nuttx /
threadx_riscv64, 1 s on threadx-linux — and only then starts the talker. So the
LATER node is the talker. The skew runs the other way, and it gives the earlier
node EXTRA observation rather than robbing the later one.

**2. There is no fixed window for skew to push a lapse past.** The run ends when
`collect_until_count` sees the LISTENER's Nth sample, and those samples cannot
arrive until the talker is up and publishing. So the later node always gets
~N seconds of its own uptime whatever the skew is. `pubsub_window` is the
TIMEOUT on that collection, not the length of the observation.

Both halves of "start skew can push one node's only lapse past the window's end"
therefore fail: the node it names is the early one, and the end it names is not
fixed.

**What is real is the MARGIN, at the top of the band.** The talker's uptime at
the assertion is ~N seconds; its first lapse is at `2L`, which for `L -> 30 s`
is `-> 59.8 s`. Against 60 samples that is a **sub-second** margin, and missing
it drops the count to 3 — a PASS on a build this cell exists to reject. That is
the same conclusion the issue reached, reached correctly.

    lease L    sessions@60    sessions@70    (limit 3)
        5          16             18
       10           9              9
       20           5              5
       25           4              4
       29           4              4
     29.9           4              4
    healthy L=60:   2 sessions, passes correctly at both

The table is the point: the verdict does not change, the MARGIN does — 0.2 s
becomes 10.2 s, about 17 % of the lapse period, against a lapse that is
timer-driven and jitters in milliseconds.

**Cost.** +10 s per cell, 12 cells, ~2 minutes total — against the ~12 minutes
the 120 s direction would have cost. What 120 s buys beyond this is a SECOND
lapse per node, i.e. tolerance of missing one entirely; no mechanism on record
misses a timer by ten seconds, so that is margin nobody has priced a need for.

**Not run here.** The cell needs QEMU and built fixtures; this change is
justified by the arithmetic above and compile-checked (`cargo check --test
rtos_e2e`). The acceptance — every `L` in `(0, 30_000)` fails the cell — is a
property of the count, and the count is unchanged; what was bought is the margin
that makes it observable.

**The per-NODE counting section stands unchanged.** The zid really is
regenerated per session (`zpico_next_session_zid_counter`), so grouping by zid
still reads as more nodes rather than more sessions. Nothing here needed it.
