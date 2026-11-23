---
id: 2886
title: "The island-shaped Zephyr snapshot the merge queue checks is stale: a live configure now derives more queryables, liveliness tokens and publishers"
status: open
type: bug
area: [ci, zephyr, sizing]
severity: high
found: 2026-11-23
related: [phase-590]
---

## What fails

The nightly's `check / template-copy-out [zephyr]` job (run 44988007873, 2026-11-22, main
`98b5d8e0c7`) fails `just check island-shaped-zephyr`:

```text
  - nros/entity_inventory.cmake: differs
      snapshot set(NROS_DERIVED_MAX_QUERYABLES 3)      live set(NROS_DERIVED_MAX_QUERYABLES 15)
      snapshot set(NROS_DERIVED_MAX_LIVELINESS 9)      live set(NROS_DERIVED_MAX_LIVELINESS 25)
      snapshot set(NROS_DERIVED_MAX_PUBLISHERS 3)      live set(NROS_DERIVED_MAX_PUBLISHERS 6)
      snapshot set(NROS_DERIVED_MAX_SUBSCRIBERS 3)     live set(NROS_DERIVED_MAX_SUBSCRIBERS 4)
      snapshot set(NROS_DERIVED_EXECUTOR_MAX_CBS 5)    live set(NROS_DERIVED_EXECUTOR_MAX_CBS 6)
island-shaped-zephyr: FAIL -- the snapshot the merge queue checks is stale
```

## Lead

The growth matches every image now serving the stock node interfaces by default (six parameter
services, each a queryable, plus `/rosout`; the `+phase-556-w13-node-interfaces-default`
fragment). Each live number has to be checked against that change before the snapshot is
regenerated; a regeneration that only makes the check pass would hide a real increase.

## Fix

`just check island-shaped-zephyr-regen` (needs a Zephyr 3.7 west workspace), then commit the
result, with the reason for each delta in the commit message.

## Acceptance

`island-shaped-zephyr` passes in a scheduled nightly. A release blocker:
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.
