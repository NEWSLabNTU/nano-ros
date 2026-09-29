---
id: 1582
title: "The Zephyr realtime-rust entry and derived-entry west leaves share a coordinate, so building both leaves the wrong image under one name"
status: open
type: bug
area: [testing, zephyr, fixtures]
severity: medium
found: 2026-09-29
related: [issue-1571, issue-1537, issue-1016]
---

## What happens (reported by the issue-1571 agent, not yet re-measured)

`build-ws-rs-realtime-entry-zenoh` and
`build-ws-rs-realtime-derived-entry-zenoh` have the same coordinate. When both
were built in one run, the realtime-entry image contained the DERIVED content:
its console printed `boot tier derived-telem_node`. As a result,
`sched_dims_applied_e2e` zephyr/rust CorePin and EdfDeadline failed on an
image that was never the one they meant to test.

The derived leaf was added by the #1537 test lane (#1431).

## Likely shape

Either two manifest rows resolve to one artifact root, or the generated-entry
path is keyed on platform + RMW rather than on the bringup/entry. The second
is the collision risk noted in the #1436 review, and it was never filed.
`check-west-leaf-vocabulary` (issue 1016) checks names, not whether two names
share an output.

## Acceptance

- The two leaves build to distinct artifacts, and each image's console names
  its own tiers.
- A gate refuses two fixture rows or west leaves that resolve to one artifact
  or one generated-entry path.
