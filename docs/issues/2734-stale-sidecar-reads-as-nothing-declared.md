---
id: 2734
title: "A stale metadata sidecar is reported as \"nothing declared\", so NROS_PROBE_FAILURE=refuse lets a build keep its default sizes"
status: open
type: bug
severity: medium
area: [sizing, cli]
related: [issue-2720, issue-2520, issue-2689, issue-1694]
found: 2026-11-19
---

**Home:** sizing. Split out of issue 2720.

## What

Measured on 2026-11-19 with issue 2520's resolve. Every sidecar of
`examples/workspaces/cpp` was made stale (its recorded digest altered).
Then `NROS_PROBE_FAILURE=refuse nros build pubsub_bringup:linux_talker
--dry-run` exited 0. The image was classified "nothing declared to derive
sizes from … State what each node creates … in the contract sidecar".

The components ARE declared: their sidecars exist and are stale. So the note
is false, and its advice (write a contract) fixes nothing.

## Fix

A stale sidecar is `NotDerived::Failed`: "the probe's counts for N
components are stale", naming the moved input that `stale_key_attribution`
reports. Under `refuse` it then refuses.

This was held back while issue 2720 made every CI sidecar stale. Nightly
44463027311 shows none are stale now, so it can land.
