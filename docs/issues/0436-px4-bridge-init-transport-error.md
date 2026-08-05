---
id: 436
title: "PX4 bridge module: `nros::init()` returns TransportError(-100) when a networked backend is registered alongside uORB"
status: open
type: bug
area: rmw
related: [issue-0362, phase-325]
---

## Symptom

The phase-325 W3 bridge module (`examples/px4/cpp/bridge`) builds, links and
registers, but fails at startup:

```
pxh> nros_uorb_bridge start
ERROR [nros_uorb_bridge] nros::init() failed: code=-100     # TransportError
ERROR [nros_uorb_bridge] Task start failed (-1)
```

It fails in `nros::init()` — BEFORE the module creates either node, so before any
of the two-session (`NodeBuilder(...).rmw(...)`) code runs.

## What is ruled out

- **Not the link.** `bin/px4` carries the module, the generated CDR symbols
  (`nros_cpp_publish_px4_msgs_msg_debug_key_value`) and BOTH backends' register
  symbols. The W3 gate (one module, two backends) still holds.
- **Not a missing router.** Same `-100` with `zenohd` listening and
  `NROS_LOCATOR=tcp/127.0.0.1:47501` exported into the PX4 process.
- **Not registration order.** The generated stub registers uORB FIRST:
  ```c
  void nros_app_register_backends(void) {
      (void)nros_rmw_uorb_register();
      (void)nros_rmw_zenoh_register();
  }
  ```
  so the default session should be the uORB one — which is exactly what the W2
  demo (`examples/px4/cpp/firmware`, `BACKENDS uorb` only) opens successfully.

## The suspicion, stated as a suspicion

The difference from the working W2 demo is that a SECOND (networked) backend is
registered. `nros::init()` opens a process-default session; with two backends
registered, something in that default-session path takes the networked transport
(or a per-backend init runs and the zenoh one fails inside PX4's posix/work-queue
context). That is a hypothesis — the error code is the only evidence so far.

A clean discriminator was attempted (rebuild with `BACKENDS uorb` alone) and is
NOT conclusive: the module publishes on the networked backend, so dropping it
fails to link for an unrelated reason. Discriminating properly needs either a
build where the module's outward half is `#if`-ed out, or tracing which backend
`nros_cpp_init` selects.

## Why it matters

This is the last step between the bridge scaffolding and a ROS 2 peer test. The
build, the link, the codegen (issue 0362) and the type hash are all verified; the
module simply cannot start.

## Direction

1. Find what `nros_cpp_init` does when MORE THAN ONE backend is registered —
   specifically which session it opens by default, and whether a networked
   backend's session-open is attempted at `init()` time.
2. If the default session is the problem, the bridge shape (two explicitly-named
   per-node sessions) wants an `init()` that opens NO default session, or one that
   names its backend — the C++ analogue of what `NodeBuilder::rmw()` already does
   per node.
