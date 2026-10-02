---
id: 1643
title: "The matched-count slots are filled by Cyclone and read by no runtime path — only by tests"
status: open
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
