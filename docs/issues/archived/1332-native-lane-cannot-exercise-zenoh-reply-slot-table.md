---
id: 1332
title: "the native host lane cannot refuse a zenoh reply slot at all, so it cannot
  be the negative control for issue 0902's leak"
status: resolved
type: limitation
area: testing
related: [issue-0902, phase-455, issue-0903, issue-1341, issue-1342]
---

## What was measured

phase-455 W2 built a live goal-completion probe
(`tests/action_multigoal.rs::every_accepted_goal_returns_a_result_and_no_reply_slot_is_refused`,
`just native test-action-completion`) and the W1 counter it reads. Both work.
The probe is green:

```
phase-455 W2 evidence: completed 3/3 after a 10000 ms soak with 3 peer up/down
cycles; server says `reply-slot: refusals=0`
```

**But the green says less than it looks like it says.** Three independent runs
on this lane, each with a router (`rmw_zenohd`), the concurrent action server
and our own peers brought up and down:

| tree under test | refusals | completed |
| --- | ---: | ---: |
| `main` (both leak arms fixed) | 0 | 3/3 |
| `1a032a10b` REVERTED (declined queries keep their slot for ever) | 0 | 3/3 |
| `1a032a10b` reverted + **10** peer up/down cycles | 0 | — |
| `-DZPICO_MAX_PENDING_REPLIES=1` (a ONE-slot table) | 0 | 3/3 |

A one-slot table that refuses nothing is the decisive row: on this lane the
action server's queryable never has more than one query in flight, so the
allocation never fails, so **the leak arm is never reached and the counter can
never move.** Reverting the fix changes no observable.

## Why (inferred from the code, not measured)

Issue 0902's leak is fed by queries the Rust callback DECLINES — the
empty-payload liveliness probe at `shim/service.rs:220-227` and the ring-full
drop below it. Two properties of the native lane appear to remove both:

* **Graph discovery here is a liveliness SUBSCRIBER, not a query.** phase-381 /
  issue 0903 replaced the per-question `z_liveliness_get` with a standing
  subscriber with history, precisely because the sweep was unreliable. A
  subscriber delivers samples, not queries, so a peer joining or leaving does
  not reach another node's queryable callback at all.
* **Both action client paths are single-in-flight.** `send_goal` and
  `get_result` each refuse a second concurrent request per client, so one
  client cannot fill even a one-slot table, and the test's peers issue no
  queries of their own.

Issue 0902 measured its 20–90 % spread on a **bare-metal serial board inside a
real ROS 2 graph**, where foreign nodes do query the queryable. That population
is what this lane lacks — not the code path.

## What this means for anyone reading a green run

`just native test-action-completion` is a real gate for the **completion path**:
it proves every accepted goal's result comes back, which the pre-phase-455
fixture never asked. It is **not** a regression gate for the reply-slot leak,
and a future change that reintroduced the leak would not turn it red. Do not
cite it as coverage for issue 0902's mechanism.

## What would close it

Any ONE of:

1. **A live ROS 2 peer that queries the action's queryable** — a
   `ros2 action send_goal` client, or `rmw_zenoh_cpp` doing its own
   service-readiness probing. phase-455 W3 adds the first Zenoh action interop
   cells (`native-action-rust-zenoh-n2r` / `-r2n`); if those peers produce
   declined queries, W3's cells become the negative control this one cannot be.
   **Measure it — do not assume it.** The cheap check is the same heartbeat:
   run the interop cell against a tree with `1a032a10b` reverted and see whether
   `reply-slot: refusals=` ever leaves 0.
2. **A fixture that declines a query on purpose** — a nano-ros node that sends
   an empty-payload query to the server's keyexpr. That reproduces the exact
   declined-query shape without a ROS graph, and with
   `-DZPICO_MAX_PENDING_REPLIES=1` it would need one query rather than four.
   This is the cheapest route and needs no ROS.
3. **The board run** — phase-444 W2's hardware measurement, which is the
   population 0902 actually measured.

## Resolution — route 2, phase-455 W2.b (2026-09-12)

`zenoh_integration::a_declined_query_hands_its_reply_slot_back`, run by
`just native test-reply-slot-decline`. One session, one real `ZenohServiceServer`
(the shipping shim callback), and `capacity + 2` **empty-payload** queries sent
to the server's own keyexpr through `Context::get_start` — which is exactly the
shape the callback's liveliness arm drops. The key is built with our own
`ServiceKeyExpr`, not hand-spelled, because a key that MISSES reads identically
to a fix that works.

**The control works, both directions.** Same host, same router, one
scratch edit — `1a032a10b`'s reclaim removed and nothing else:

| `ZPICO_MAX_PENDING_REPLIES` | tree | verdict | counters |
| ---: | --- | --- | --- |
| 4 (shipped) | reclaim present | **PASS** | `started=6 finalised=6 declines=6 held=0 refusals=0` |
| 4 (shipped) | reclaim REVERTED | **FAIL** | `started=4 finalised=0 declines=4 held=4 refusals=0` |
| 2 | reclaim present | **PASS** | `started=4 finalised=4 declines=4 held=0 refusals=0` |
| 2 | reclaim REVERTED | **FAIL** | `started=4 finalised=2 declines=2 held=2 refusals=2` |

Three things that reading could not have told us:

* **The refusal counter is the wrong primary observable on the shipped build,
  and `held` is the right one.** `ZPICO_MAX_PENDING_REPLIES` (4) is also the
  querier's `ZPICO_MAX_PENDING_GETS` (4), and a leaked slot holds the cloned
  query for ever, so the query never finalises and the CLIENT's pending-get slot
  is never released either. The querier runs dry at exactly four — before a
  fifth query can be refused. `held=4/4` is what the leak looks like there;
  `refusals` only moves once the reply table is SMALLER than the querier's pool,
  which the `capacity=2` row shows (`refusals=2`). Both are asserted; `held` is
  what actually goes red on a default build.
* **A refused query is not a declined one.** It never receives a reply seq, so
  `query_handler`'s decline arm cannot see it. The arrival count — the positive
  control that stops this test being another green that cannot fail — is
  `declines + refusals`, which the `capacity=2` reverted row proved: four
  queries read `declines=2 refusals=2`, and an assertion written as
  `declines >= started` called that a probe that had MISSED.
* **This lane really did produce zero declined queries before.** With the probe
  removed the counter stays at 0 for the whole run, which is the "Not verified"
  note below, now measured rather than inferred.

### What the probe does NOT reach

* **The ring-full drop arm.** An empty payload returns ABOVE the ring check, so
  the probe never fills the request ring. Measured with a scratch payload-carrying
  variant (`ZPICO_MAX_PENDING_GETS=16`, server never draining, 12 queries):

  | `ZPICO_MAX_PENDING_REPLIES` | result |
  | ---: | --- |
  | 4 (shipped) | `declines=0 held=4 refusals=8` |
  | 8 | `declines=8 held=4 refusals=0` |

  At the shipped sizes `SERVICE_REQUEST_RING_DEPTH` (4) **equals**
  `ZPICO_MAX_PENDING_REPLIES` (4), and an enqueued request consumes one of each,
  so the two saturate on the same query: a query that meets a full ring has
  already been refused a slot and holds nothing to reclaim. The ring-full arm's
  reclaim is therefore unreachable at default sizes and becomes live only on an
  image that raises the reply table above the ring depth (`capacity=8` row,
  `declines=8`). Worth its own issue if anyone wants that arm gated; it is not a
  leak today, because there is no slot to leak.
* **`b56e3d50a`'s arm, which is a different one.** That commit fixed the FAILED
  REPLY path — four error returns inside `zpico_query_reply` that left the slot
  held — not the ring-full drop. This probe never makes the server reply, so it
  never enters `zpico_query_reply` at all. Controlling that arm needs a reply
  that fails (a bad keyexpr, an oversized payload, or a `z_query_reply` failure),
  which is a different fixture.
* **Route 1 stays open and stays stronger.** A live `rmw_zenoh_cpp` peer is the
  population 0902 measured; this probe is a synthetic stand-in for one shape of
  it. The host these runs were made on has no ROS, so whether W3's interop cells
  produce declined queries is still unmeasured — do not assume it.

### Also worth knowing about the runs above

The host had no ROS, so `rmw_zenohd` does not exist on it and
`NROS_RMW_ZENOHD` was pointed at a shim translating the fixture's
`ZENOH_CONFIG_OVERRIDE` into the standalone SDK `zenohd`'s argv. Per RFC-0075
that says nothing about the ROS-paired configuration — but nothing in this probe
is about a ROS peer: both ends are ours, on one session, and the router only has
to let the session open.

## Not verified

The "why" above is read off the code and is consistent with all four rows of
the table, but nothing here instrumented the queryable callback to confirm that
zero declined queries arrive. A counter of DECLINED queries beside the refusal
counter would settle it in one run, and is the obvious follow-up to option 2.

*(That counter is `zpico_reply_slot_declines` / `Context::reply_slot_declines`,
landed with the resolution above. It reads 0 on this lane without the probe.)*

## Route 1, MEASURED 2026-09-12 — a live `rmw_zenoh_cpp` peer declines nothing

The "What would close it" list says of route 1: *"if those peers produce
declined queries, W3's cells become the negative control this one cannot be.
**Measure it — do not assume it.**"* It was measured. **They do not.**

The route-1 recipe as written could not be run: it says to run W3's interop cell
against a tree with `1a032a10b` reverted, and on `main` the r2n cell's server
does not start at all — its `/fibonacci/_action/status` publisher asks for
TRANSIENT_LOCAL and the zenoh shim refuses it since `b0ea5a04b` (issue 1341).
So the question was put to the same queryable one layer down: a nano-ros zenoh
SERVICE server, which is the same zenoh-pico queryable an action server is.

Instrument: an experiment-only `-DZPICO_DECLINE_PROBE` in `query_handler`
printing one line PER QUERY with the decline verdict — per query rather than per
decline, so an answered request is the instrument's own control. Population, all
on one ephemeral `rmw_zenohd`, ~110 s: a fresh ros2cli daemon, `demo_nodes_cpp`
talker and listener, three rounds of `ros2 node list` / `service list` /
`topic list` / `node info`, four real `ros2 service call`, and a 60 s idle soak
with the graph present.

```
queries seen: 4  declined: 0
ZPICO_QUERY n=1 declined=0 declined_total=0 payload_len=20 keyexpr=0/add_two_ints/…
… n=2, n=3, n=4 identical
```

Four queries in 110 s — one per service call, and nothing else. `payload_len=20`
on every one, so the empty-payload arm was never entered; a separate 12-way
concurrent `ros2 service call` flood answered 12/12 with 0 declines, so the
ring-full arm was not entered either.

**So the "Why (inferred from the code, not measured)" section above is now
measured, and it is stronger than it was written.** A real `rmw_zenoh_cpp` peer
population does not merely fail to fill the table: it sends our queryable
nothing but the requests it means to send. The resolution's route-2 probe is the
control this lane has, and it is the only one it is going to get from a host
lane — phase-444 W2's board run is what remains for the population issue 0902
actually measured.

(The closing note above says `zpico_reply_slot_declines` "reads 0 on this lane
without the probe". That is now also true with a live ROS 2 graph attached, and
for the same reason: nothing declines.)
