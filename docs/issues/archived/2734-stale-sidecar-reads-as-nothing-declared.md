---
id: 2734
title: "A stale metadata sidecar is reported as \"nothing declared\", so NROS_PROBE_FAILURE=refuse lets a build keep its default sizes"
status: resolved
resolved_in: "the change that archives this issue"
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

`metadata_refresh::probe_rows_and_stale` reports each stale sidecar it
skips, with its package and the moved input. `stale_launched_reason`
turns the ones the model LAUNCHES into one reason: "the probe's counts for
N launched component(s) are stale: ... Moved: ...". A stale sidecar of a
package the image does not launch is not its failure.

`contract_inventory` returns it as `Err` in both arms:

- no contract, and the probe states nothing: the counts are declared and
  could not be read;
- a contract, floored by the probe: a stale floor is a missing floor, so
  the contract alone may under-count what the code creates (issue 1694).

Every caller reports it under `NROS_PROBE_FAILURE`:

- the build's resolve, as `NotDerived::Failed`;
- the configure-time composers (`nros ws entity-inventory`'s `with_model`
  and `fold_models_for_shared_runtime`) and the sizing descriptor's model
  road, through `contract_inventory_reported`: a warning, or under
  `refuse` an error.

It could land now because nightly 44463027311 shows no stale sidecar in any
fixture job (issue 2720).
