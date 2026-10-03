---
id: 1643
title: "The matched-count slots are filled by Cyclone and read by no runtime path — only by tests"
status: resolved
resolved_in: 2026-10-03
type: tech-debt
area: rmw
severity: low
found: 2026-10-02
related: [1617, phase-393, phase-433, phase-472]
---

## What

`publisher_count_matched_subscriptions` and
`subscription_count_matched_publishers` are upstream-parity vtable slots
(phase-393 W2). Cyclone implements both. Their only readers are tests:

- `packages/testing/nros-tests/bins/advertised-state-probe`, which drives the
  vtable directly against a stock ROS peer;
- `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/graph_counts.cpp`.

No nros node API exposes a matched count, so an application cannot ask.

## How it was found

Issue 1617 (phase-472 W8) stopped `check-rmw-slot-producers` from counting a
test reader as a consumer. A test exercising a slot proves the slot answers,
not that anything that ships asks. Both slots then classified **inert**. They
are recorded in the gate's `matched-counts` family and marked
`not-implemented` in `docs/reference/rmw-api-map.toml` against this issue.

## What closing needs

Pick one:

- expose matched counts on the publisher/subscription API, so a runtime path
  consumes the slot (the family entry and the `not-implemented` row then go,
  and the gate refuses them as stale); or
- rule them ABI-only parity surface (`not-supported`, with the reason).

## Resolution (2026-10-03)

The first option: the slots got a production reader.

- `nros_rmw::Publisher::count_matched_subscriptions` and
  `nros_rmw::Subscription::count_matched_publishers` are new trait methods.
  They default to `Err(Unsupported)`, never a fabricated `0`, the same
  discipline as `get_gid`. `CffiPublisher` / `CffiSubscription` forward them
  to the `publisher_count_matched_subscriptions` /
  `subscription_count_matched_publishers` vtable slots, and a NULL slot
  answers `Unsupported`.
- The node API's `get_subscription_count` / `get_publisher_count` (phase-444)
  now ask the backend's matched count first. That is upstream rclcpp's exact
  question, answered on Cyclone. They fall back to the topic-wide graph count
  only on `Unsupported` (zenoh, XRCE, uORB). A matched `0` is an answer and
  does not fall through.
- New unit test `a_backend_matched_count_wins_over_the_graph_count`. It fails
  when `get_subscription_count` is reverted to graph-only (measured).
- The `matched-counts` family in `check-rmw-slot-producers` was removed: the
  gate refused it as stale once both slots classified `produced`. The two
  `not-implemented` rows in `docs/reference/rmw-api-map.toml` went too, and
  the book's RMW API comparison page was regenerated.

No C header changed, so the generated bindings did not need regenerating.
`check abi-bindings`, `check rmw-api-parity`, `check rmw-abi-shape` and
`check rmw-slot-producers` are green.

