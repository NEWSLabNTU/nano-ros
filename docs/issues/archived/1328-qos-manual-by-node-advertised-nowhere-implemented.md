---
id: 1328
title: "`LIVELINESS_MANUAL_BY_NODE` was advertised by every backend and implemented
  by none"
status: resolved
type: bug
area: rmw
severity: medium
related: [1329]
---

## What happened

`Session::supported_qos_policies()` advertised `LIVELINESS_MANUAL_BY_NODE` on
both masks in the tree — the zenoh shim and the cffi route — and no backend
implemented the policy.

Measured 2026-09-11, phase-428 W9:

| backend | what it does with `MANUAL_BY_NODE` |
| --- | --- |
| zenoh | `shim/publisher.rs` matches `ManualByTopic \| ManualByNode` in one arm, so assertion is per PUBLISHER |
| cyclonedds | `src/qos.cpp` folds it onto `DDS_LIVELINESS_MANUAL_BY_TOPIC`, with a comment saying so |
| xrce | lowers no liveliness field at all — `uxrQoS_t` has four fields and none is liveliness |
| uorb | reads no QoS field whatever |

## Why it matters

The two policies differ in WHO has to assert. `MANUAL_BY_TOPIC` requires the
application to assert each publisher; `MANUAL_BY_NODE` says an assertion on any
of a node's publishers keeps the node's whole set alive. Serving the second as
the first is a downgrade in the LOSING direction: an application that asserts
once per period, correctly, watches its other publishers cross the lease and
report `LivelinessLost` — and its subscribers see them drop out of the graph.

That is the exact failure the mask exists to prevent. The contract is "no
silent downgrade: refuse at create instead", and the mask said the policy was
available.

Cyclone's fold is the sharper case, because it is what meets real ROS peers:
upstream `rmw_cyclonedds_cpp` has the same fold, but it does not then tell the
application that MANUAL_BY_NODE is supported.

## Fix (landed, phase-428 W9)

* The bit is withdrawn from both masks, so a request is refused with
  `IncompatibleQos` at create.
* The zenoh backend refuses it locally too (`shim/qos.rs`), for a caller who
  reached the backend without the runtime's pre-validate step.
* `check-qos-mask-derivation` makes the class un-repeatable: a bit may be
  advertised only if a `nros-qos-honours:` claim is sited on code that reads
  the mapped field. Re-advertising `MANUAL_BY_NODE` is one of the gate's own
  live-tree mutation controls, so the red is re-demonstrated on every run.

Nothing in the tree requested the policy, so the withdrawal changed no working
build. Implementing it for real (per-node assertion tracking in the zenoh
liveliness layer) is a separate piece of work, and the honest mask is the
precondition for anyone noticing it is missing.
