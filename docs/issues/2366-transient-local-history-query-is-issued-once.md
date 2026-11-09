---
id: 2366
title: "A transient-local subscription's history query is issued once: a latched publisher that appears later, or a session re-open, is not re-read"
status: open
type: enhancement
area: [rmw-zenoh, interop]
severity: low
found: 2026-11-09
related: [1806, 1341, phase-473, phase-512]
---

**Home: [phase-512](../roadmap/phase-512-on-hardware-measurements.md)**: split out
of [issue 1806](archived/1806-zenoh-constrained-link-unvalidated-on-full-autoware-graph.md)
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
  lease without traffic ([issue 0913](archived/0913-the-debugger-is-not-a-passive-instrument.md)).

It matters for a publisher that latches a value and never changes it, read by
a nano-ros node that joined first or that lost its session in between.

## Closes when

The subscription re-queries the history of a publisher that appears after it
(by the `@adv` liveliness token, which means a liveliness subscriber the
graph-discovery-off build otherwise avoids, so the cost has to be measured on
a serial image), and after a session re-open; or a decision records that
neither is done and why.
