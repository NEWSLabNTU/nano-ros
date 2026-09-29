---
id: 1572
title: "A refused transient-local count sized ZERO cache queryables and a builtin retention pool, which is the unsafe direction"
status: resolved
type: bug
area: [cli, sizing, zenoh]
severity: high
found: 2026-09-29
related: [1341, 1378, 1567]
resolved_in: "branch fix/refused-tl-count-fails-safe"
---

## What

`nros_sizing_descriptor::transient_local_publishers_over` REFUSES the count
when a publisher row states no durability. The rule is right about that: a
count over the rows that answered puts no upper limit on the silent row. But
every consumer then sized as if the answer were zero:

| road | consumer | on a refusal it sized |
| --- | --- | --- |
| entity inventory (Zephyr west, CMake) | `EntityInventory::derive` -> `NROS_DERIVED_MAX_QUERYABLES` | 0 cache queryables |
| entity inventory (Zephyr west) | `NROS_DERIVED_TL_PUBLISHERS` -> retention pool | not published, so builtin 2 |
| declared carrier (CMake / NuttX) | `nros-zpico-build` `NROS_DECLARED_TL_PUBLISHERS=refused` | 0 |
| declared carrier | `nros-rmw-zenoh` retention pool | builtin 2 |
| sizing descriptor (cargo leaf) | `nros-zpico-build` | 0 |
| sizing descriptor | `nros-rmw-zenoh` retention pool | builtin 2 |

The silent publisher is the row the refusal is about, and if it is latched,
it needs the slot the zero withheld. Its `create_publisher` fails with
`Full` at boot, which the C++ ABI reports as code -3.

## Measured

Autoware Safety Island (Zephyr 4.4, west, `mps2/an385` under QEMU), phase-8
W13, recorded in issue 1567. A fifth row that states no durability refused
the count, `NROS_DERIVED_MAX_QUERYABLES` derived to the two services alone,
and the first latched publisher failed:

```
[ERROR] .../nros/node.hpp:332 node "mrm_comfortable_stop_operator": FAILED at create_publisher_in (code=-3)
```

Issue 1567 removed the row that triggered it. It did not change the rule that
turned a refusal into zero.

## Fix: size for the worst case, do not refuse the configure

RFC-0100 D6 already covers this case: *"Worst case when refused, always the
safe direction and always loud"*. For XRCE that rule means "assume reliable",
and here it means "assume transient-local". The two options were:

* **Refuse the configure, naming the row.** This is safe, but it breaks every
  image with a silent VOLATILE publisher, and most images have one. An image
  that boots today would stop building, all to protect against a failure that
  only a latched silent publisher can cause.
* **Size for the worst case (chosen).** Count every publisher that states no
  durability as transient-local, plus the action servers the rule already
  counts. The result is an upper bound: the true count can only be lower,
  and only by the silent rows that turn out volatile. A pool sized from it
  cannot fall short, and no image that boots today stops building. The
  over-count costs one queryable-table entry and one retention slot per
  silent row, and stating the row's durability gives those back. The
  refusal prose, which names the row, still reaches the build output.

Changes:

* `nros_sizing_descriptor::transient_local_publishers_bound_over` /
  `transient_local_publishers_bound` compute the worst case over the same rows
  as the rule. `parse_declared_tl` / `declared_tl_token` are the carrier's one
  spelling, which both readers share.
* The declared carrier sends `refused:<worst case>` instead of the bare word.
  `NanoRosEntityFacts.cmake` takes the MAX of the worst cases and the exact
  counts and forwards the word with the number. A bare `refused` comes from
  an older CLI and carries no number, so it now FAILS the build and says to
  rebuild the CLI. It no longer sizes zero.
* `EntityInventory::derive` adds the worst case to `max_queryables`
  (`DerivedEntityKnobs::tl_slots`). The fragment publishes it as
  `NROS_DERIVED_TL_PUBLISHERS`, with a comment that says it is a worst case and
  names the silent row.
* `nros-zpico-build` and `nros-rmw-zenoh` size from the worst case on both the
  descriptor road and the carrier road, and each prints a `cargo:warning`
  that says so.

## Tests

* `nros-sizing-descriptor`: `a_refused_count_sizes_its_pools_from_the_worst_case`,
  `the_declared_carrier_carries_the_worst_case_with_the_refusal`.
* `nros-cli-core`: `a_silent_publisher_sizes_the_transient_local_pools_for_the_worst_case`
  (replaces `a_silent_publisher_refuses_the_transient_local_count`, which
  asserted the old behaviour). With `derive` put back to the old zero rule, it
  fails:

  ```
  assertion `left == right` failed: the worst case counts the silent row
    left: 0
   right: 2
  ```

  Six existing inventory/resolve tests used fixtures with a publisher that
  states no durability and asserted the zero. They now assert the worst-case
  slot explicitly. Also new: `declared_entities_bound_a_refused_transient_local_count_from_above`,
  `a_silent_publisher_sends_the_worst_case_with_the_refusal`.
* `nros-zpico-build`: `the_declared_road_supplies_the_same_term_when_there_is_no_descriptor`
  now expects `refused:2` -> 2, and `a_bare_refusal_with_no_worst_case_is_a_build_failure`
  is new.
* End to end through both build scripts, with
  `NROS_DECLARED_SERVICE_SERVERS=2 NROS_DECLARED_INFRA_QUERYABLES=none NROS_DECLARED_TL_PUBLISHERS=refused:3 cargo check -p nros-rmw-zenoh`:
  `ZPICO_MAX_QUERYABLES: usize = 5` and `MAX_TL_PUBLISHERS: usize = 3`, with
  both warnings printed. A bare `refused` panics with
  `NROS_DECLARED_TL_PUBLISHERS=refused carries no worst case`.

## Not fixed here

* The MODEL road's `NROS_DECLARED_TL_PUBLISHERS` (`facts_from_model`) still
  counts action servers only. A model carries no QoS, so a hand-written
  transient-local topic publisher there is invisible to it. That is issue
  1393's gap (no model-only descriptor producer), and the entity inventory,
  which does read the contract, is the road that sizes the Zephyr image.
* On the Zephyr road, the worst case disagrees with the durability table's
  row count, so `ZPICO_TL_RETAIN_BYTES` keeps its builtin slot size. That is
  the safe direction, and the STATUS line gives the reason.
