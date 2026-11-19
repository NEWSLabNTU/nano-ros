---
id: 2755
title: "On mps3-an536-freertos with Cyclone DDS a C++ node's parameter services fail at describe_parameters with `rmw_ret error`"
status: open
type: bug
severity: medium
area: [rmw, params, freertos]
related: [2704, 1268]
found: 2026-11-19
---

## Measured

nano-ros 93368ca929, the downstream island's controller image
(`controller_bringup:an536`, board `mps3-an536-freertos`, `rmw =
"cyclonedds"`, `features = ["param_services"]`, 164 parameters, one node
`/controller`), branch `nano-ros-next`. `nros run controller_bringup:an536`
on QEMU:

```
[INFO] nros: [    9.002000] parameter store: 164 slots (1389408 B) + state and services (61400 B) carved from the executor backing
...
[INFO] nros: [    9.084000] Actuation Safety Island is Live
[ERROR] nros: [    9.107000] parameter services could not be created for node /controller at service describe_parameters: Transport(Backend("rmw_ret error")). `ros2 param` cannot reach 1 of this image's 1 node(s) ...
[ERROR] nros: [    9.107000] nros: NodeError::Transport(Backend("rmw_ret error")): transport error: backend error: rmw_ret error
```

The image runs on without its parameter services. The same bringup on
`freertos-posix` (host Cyclone) creates them. Unlike issue 2704 (Zephyr,
`Transport(Unsupported)`, no descriptors) the failure is a backend error at
the second service, so `get_parameters` was created.

## Cause

Not traced. Candidates: a Cyclone entity or reader/writer pool on the lwIP
board sized below six services plus the node's 7 subscriptions and 3
publishers, or the `describe_parameters` reply type exceeding a buffer the
board sizes smaller than the host.

## Direction

Name the exhausted resource in the error rather than `rmw_ret error`, and
size it from the image's declared services (the entity inventory already
counts them).
