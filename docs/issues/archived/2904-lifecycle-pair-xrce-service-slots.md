---
id: 2904
title: "An owned C/C++ entry that makes several nodes managed in its own `main` derives too few XRCE service-server slots, and the C++ node carries on after its lifecycle services fail"
status: resolved
type: bug
severity: medium
area: [xrce, sizing, lifecycle]
related: [phase-554, issue-2482, issue-2553]
found: 2026-11-23
resolved: 2026-11-27
resolved_in: "#4795 (fe007626ed): [lifecycle] managed_nodes declares the count; a LifecycleNode reports failed services"
---

## What happens

The `lifecycle-pair` examples (C and C++) bind two nodes in their own `main`
and make each managed. Built for XRCE (`[image.linux_xrce]`), each image
reports at boot:

```text
[ERROR] nros_rmw_xrce: no free service server slot for '/lc_alpha/get_transition_graph':
  this image was BUILT with NROS_XRCE_MAX_SERVICE_SERVERS=4. ...
```

Two machines need 10 (five lifecycle services each). The C program stops
(`make_managed -> -3`). The C++ program logs
`nros_cpp_register_lifecycle_services_on: the lifecycle services could not be
created`, then prints `Lifecycle services registered on /lc_alpha` and keeps
running with no services, so `ros2 lifecycle nodes` lists nothing.

## Why

Two gaps, either one enough:

1. No sizing descriptor is written for these owned-entry cmake images, so
   `nros-rmw-xrce-cffi`'s derivation returns before the runtime term and the
   header's 4 stands (issue 2482's no-fact branch).
2. Even with one, the inventory cannot see managed nodes created in `main`:
   `lifecycle_machines` prices one machine for `features = ["lifecycle"]` with
   no node marked, i.e. 5 + the header's 4 = 9 < 10.

And separately, `rclcpp::LifecycleNode`'s constructor swallows the
registration failure. Stock rclcpp throws there.

## Workaround in tree

Both examples state `[image.linux_xrce.knobs.xrce] max_service_servers = 10`.

## Acceptance

An owned entry can declare how many managed nodes it creates, or the inventory
counts them, so the XRCE pool derives 10 without a stated knob. And the C++
lifecycle node refuses (or reports through its constructor's result) when its
services cannot be created.

## Ruling and fix (2026-11-24)

User ruling: option A, a DECLARED count, feeding the one rule both consumers
already read. No second sizing rule.

**The declaration.** `[lifecycle] managed_nodes = N` in `system.toml`.
- Accepted by the typed `[lifecycle]` table (`SystemLifecycle`).
- Read by `nros_orchestration_ir::leaf_system` (`LeafSystem::
  lifecycle_managed_nodes`). It needs `lifecycle` in a `features` list and
  must be at least 1; either mistake is refused, naming why.

**One rule.**
- `InfraServices::with_declared_managed_nodes` sets `managed_nodes` to the
  larger of the model's and the declared count, and turns the family on.
- `lifecycle_machines()`, and from it the XRCE service-server pool and the
  per-node heap floor (`NROS_ENTITY_LIFECYCLE_MACHINES`), see N machines.
- The declaration is applied on every road that builds the image's infra from
  its bringup: the leaf declaration, the owned entry's census, and the
  undeclared (Unwired) descriptor. On that last road the declared nodes also
  count as nodes.

**Gap 1, the host owned entry.** A host (posix) image is the census's own
producer, so nothing sized it. When its bringup declares the count, the
generated cmake root calls `nros_record_owned_declared_facts`, which writes
the descriptor from the declaration alone, before every reader. A bringup
that declares nothing is unchanged.

**The examples.** Both `lifecycle-pair` examples now declare
`managed_nodes = 2` and drop the `max_service_servers = 10` workaround.
Measured, the C example's `linux_xrce` image derives
`NROS_XRCE_MAX_SERVICE_SERVERS=26`, where the header's default was 4 and two
machines need 10.

**The C++ half.** `rclcpp_lifecycle::LifecycleNode` latches a failed
service registration on the node, so `ok()` goes false and `error_what()`
names it, as a failed `rclcpp::Node` construction does. The C++ example
checks `ok()` and stops, as the C one does.

**Tests.**
- The parser: read, refused without `lifecycle`, refused below 1.
- The helper: N machines, never lowering a marked count, 0 is no statement.
- The cmake-root render.
- The `lifecycle_pair_interop` cells run the examples without the knob.

The user guide's managed-nodes page states the key.

## Resolution (2026-11-27)

Fixed by #4795 (`fe007626ed`, 2026-11-24), as the section above describes.
The issue stayed open because its acceptance cells had never run: the
`lifecycle_pair_interop` cells are in the live-peer lane only once a verdict
is recorded for them, and none had been, so CI never ran them
(`check-interop-verdicts.py --report` listed all six as NEVER).

They were run by hand on host 241 at main `5067703e37`, from fixtures built
there (`just linux test-ros2-lifecycle-pair`), with both examples at
`managed_nodes = 2` and no `max_service_servers` knob:

| case | result |
| --- | --- |
| `lifecycle_cli_answers_as_stock_lifecycle_talker` C, C++ | pass, 88 to 89 s |
| `two_managed_nodes_are_silent_until_each_is_active` C, C++ on zenoh | pass, 58 s |
| the same on Cyclone DDS | pass, 150 s |
| the same on XRCE (the image this issue was about) | pass, 147 to 148 s |

9 of 9 tests passed in 445 s. Recording these verdicts in the ledger, which
would put the cells in the live-peer lane, is a separate decision: it widens
a lane whose row count is capped.
