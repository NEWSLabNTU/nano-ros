---
id: 1332
title: "the native host lane cannot refuse a zenoh reply slot at all, so it cannot
  be the negative control for issue 0902's leak"
status: open
type: limitation
area: testing
related: [issue-0902, phase-455, issue-0903]
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

## Not verified

The "why" above is read off the code and is consistent with all four rows of
the table, but nothing here instrumented the queryable callback to confirm that
zero declined queries arrive. A counter of DECLINED queries beside the refusal
counter would settle it in one run, and is the obvious follow-up to option 2.
