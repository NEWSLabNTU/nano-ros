---
id: 2366
title: "A transient-local subscription's history query is issued once: a latched publisher that appears later, or a session re-open, is not re-read"
status: resolved
type: enhancement
area: [rmw-zenoh, interop]
severity: low
found: 2026-11-09
resolved: 2026-11-21
resolved_in: "fix(rmw-zenoh, issue 2366): a transient-local subscription re-reads a publisher cache when its token appears"
related: [1806, 1341, 2774, 2776, phase-473, phase-512]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)**: split out
of [issue 1806](1806-zenoh-constrained-link-unvalidated-on-full-autoware-graph.md)
("Also not done") when that issue closed on the board.

## What the code does

`query_history` (`nros-rmw-zenoh/src/shim/subscriber.rs`) is called from one
place, subscription creation. It sends the GLOBAL `<key>/@adv/**` query that
`rmw_zenoh_cpp` 0.1.9's advanced subscriber sends at creation, and the
replies are every cache that exists at that moment.

Two cases are not covered:

1. **A latched publisher that appears after the subscription.** A stock
   advanced subscriber also watches each publisher's `@adv` liveliness token
   and queries the new publisher's cache when it appears. Ours does not, so
   the new publisher's value reaches us only if it is published live after
   the routers have propagated our subscription. phase-473 left this out of
   scope.
2. **A session re-open.** zenoh-pico's reconnect (`_z_reopen`) replays the
   declarations; the history query is a get, not a declaration, and nothing
   re-issues it. A latched value that changed while the session was down is
   not seen until the publisher changes it again.

## Does it matter for the safety island

Not measurably today:

* The island's one TRANSIENT_LOCAL subscription is `mrm_handler`'s
  `/api/operation_mode/state`. In the board acts the island usually boots
  before Autoware (case 1); its handler still reaches RUN (nl-pe1,
  2026-11-07: `init failure cleared at uptime 9667 ms`), because default_adapi
  publishes the mode live once it is up.
* The same topic is republished often: during run z1806-lj1 (2026-11-09) a
  host reader saw it about 5 times a second once the route was set, because
  its `is_autonomous_mode_available` field toggled. A missed value is
  replaced within 200 ms of that point.
* Case 2 needs a session loss; on the board that takes more than the 60 s
  lease without traffic ([issue 0913](0913-the-debugger-is-not-a-passive-instrument.md)).

It matters for a publisher that latches a value and never changes it, read by
a nano-ros node that joined first or that lost its session in between.

## Closes when

The subscription re-queries the history of a publisher that appears after it
(by the `@adv` liveliness token, which means a liveliness subscriber the
graph-discovery-off build otherwise avoids, so the cost has to be measured on
a serial image), and after a session re-open; or a decision records that
neither is done and why.

## A third case, measured: a nano-ros publisher racing its own route

Measured on 2026-11-20 against main, with a nano-ros TRANSIENT_LOCAL
subscription up first and a nano-ros TRANSIENT_LOCAL publisher that publishes
once right after it is created (`constrained-link-probe`, graph discovery off,
one `rmw_zenohd`): the value was **lost in 4 of 5 runs**, with no error on
either side. A zenoh-pico publisher in a threaded build opens with its write
filter ACTIVE (`filtering.c`) until the router answers its interest, so that
first live sample reaches nobody; the cache holds it, and nothing queries the
cache again. A stock `rclpy` latched publisher in the same position delivered
live in 3 of 3, so the loss is ours, between two nano-ros nodes.

## Resolution

Both halves of the stock advanced pub/sub's late-publisher detection:

* A transient-local publisher declares a liveliness token on its cache
  queryable's own key, `<topic keyexpr>/@adv/pub/<zid>/<eid>/_`, where a stock
  advanced publisher declares it. It is heap, pointed to by the queryable's
  slot (`queryable_entry_t::adv_token`, NULL for every other queryable), so it
  takes no slot of the `ZPICO_MAX_LIVELINESS` table, and goes with the
  queryable.
* A transient-local subscription declares a liveliness subscriber on
  `<topic keyexpr>/@adv/pub/**`, with history. It and the rest of the
  subscription's transient-local state are one heap block
  (`struct zpico_tl_sub`), allocated only for a transient-local subscription
  and pointed to by its slot, so an image without one pays a NULL pointer per
  subscriber slot and nothing else.
  A token's PUT only sets a flag; the next `zpico_spin_once` sends the history
  query again on the caller's thread (never from zenoh-pico's receive path,
  where the declare lock is not taken). A burst of tokens costs one query.
  Because the watch has history, a session re-open, which replays it, delivers
  every live token again, and every cache is read again.
* The dedup is per publisher and lasts the subscription's life: a table of
  `ZPICO_TL_DEDUP_WRITERS` (4) GIDs with the newest sequence number from each,
  in that heap block. It was one (GID, seq) pair, inline in every subscriber
  slot and open only during the first query's window, so a second read of a
  cache would have delivered its value twice.
* The block is reference counted under the slot lock: the subscription holds
  one reference and each history query still out holds one, released by the
  query's dropper (zenoh-pico runs it exactly once, on every path), so a late
  reply never touches a freed block or a reused slot.

Nothing counts the new declarations in a fixed table, so no pool derivation
moves, and an image with no transient-local endpoint gets smaller. Measured:

| | before | after |
| --- | --- | --- |
| `subscriber_entry_t`, each of `ZPICO_MAX_SUBSCRIBERS` slots (x86_64) | 144 B | 112 B |
| `queryable_entry_t`, each of `ZPICO_MAX_QUERYABLES` slots (x86_64) | 64 B | 72 B |
| the same two on thumbv7m | | 56 B / 36 B |
| native TL subscription, peak heap (massif, twice) | 111,674 B | 113,605 B (+1,931) |
| native TL publisher, peak heap (massif, twice) | 112,951 B | 113,941 / 114,341 B (+990 / +1,390) |
| mps2-an385 bare-metal serial talker (thumbv7m) text / bss | 463,052 / 283,216 | 465,384 / 283,216 (+2,332 / 0) |
| mps2-an385 bare-metal serial listener (thumbv7m) text / bss | 461,044 / 412,176 | 463,372 / 412,144 (+2,328 / -32) |

The serial images declare no transient-local endpoint, so their delta is what
every image pays: code, and the slots' net change. A first version kept this
state inline in every subscriber slot (144 -> 288 B, +320 B bss on the serial
listener); it moved to the heap so that a board short of RAM pays nothing for
a feature it does not use. The heap of a transient-local endpoint on a serial
board, and its link traffic, are [issue 2776](2776-tl-adv-cost-on-serial-board.md).

Measured after the fix:

* nano-ros publisher after the subscription: **10 of 10** delivered exactly
  once (4 of 5 lost before). A stock `rclpy` latched publisher after the
  subscription: 3 of 3 exactly once, no duplicate from the live sample and the
  cache reply both arriving.
* Session re-open, two routers, the publisher on A and the subscription on B,
  B stopped while the publisher changes the value and started again: **3 of 3**
  read the new value exactly once, and never the old one twice; **0 of 3**
  before the fix.
* `interop_e2e::interop::case_13` gains phase 4, the late publisher, asserting
  exactly one delivery and that it arrived after the publisher started; 3 of 3
  green.

## Not covered

The re-open case is not a cell phase. Every router stop the test harness makes
leaves a zenoh-pico client reading end-of-stream forever without reopening
(issue 2774, pre-existing: the unfixed build does the same), so the phase would
measure that. It belongs to 2774's fix.
