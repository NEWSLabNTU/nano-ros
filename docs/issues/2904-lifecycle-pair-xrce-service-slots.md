---
id: 2904
title: "An owned C/C++ entry that makes several nodes managed in its own `main` derives too few XRCE service-server slots, and the C++ node carries on after its lifecycle services fail"
status: open
type: bug
severity: medium
area: [xrce, sizing, lifecycle]
related: [phase-554, issue-2482, issue-2553]
found: 2026-11-23
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
