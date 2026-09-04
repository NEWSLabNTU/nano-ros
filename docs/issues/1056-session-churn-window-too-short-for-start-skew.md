---
id: 1056
title: "`assert_no_session_churn` can PASS on a broken lease, because start skew
  can push one node's only lapse past the 60 s window"
status: open
type: bug
area: testing
severity: medium
found: 2026-09-04
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
