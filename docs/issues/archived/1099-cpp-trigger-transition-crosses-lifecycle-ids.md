---
id: 1099
title: "`LifecycleNode::trigger_transition(uint8_t)` takes upstream's exact
  signature over a DIFFERENT id space — `2` activates where ROS 2 cleans up"
status: resolved
resolved: 2026-09-06
type: bug
area: api, cpp
related: [phase-428, rfc-0089]
---

## Problem

`nros-cpp/include/nros/lifecycle.hpp:173` passes the caller's raw `uint8_t`
straight to `nros_cpp_lifecycle_change_state`. Our `LifecycleTransition`
(`nros-core/src/lifecycle.rs:61-78`) and
`lifecycle_msgs/msg/Transition.msg` disagree on **four of eight** ids —
verified against `/opt/ros/humble/share/lifecycle_msgs/msg/Transition.msg`:

| id | ours | upstream |
| --- | --- | --- |
| 1 | Configure | CONFIGURE ✓ |
| **2** | **Activate** | **CLEANUP** |
| **3** | **Deactivate** | **ACTIVATE** |
| **4** | **Cleanup** | **DEACTIVATE** |
| 5–7 | Shutdown{Unconfigured,Inactive,Active} | ✓ |
| **8** | **ErrorRecovery** | **DESTROY** |

The header comment says *"REP-2002 transition ids"* — the standard it does not
implement.

**A ported file calling `trigger_transition(2)` on an Inactive node silently
ACTIVATES it and returns `Result::ok`,** where ROS 2 would clean it up. Same
name, same arity, same type, opposite effect: RFC-0089's compile-and-differ,
in the most dangerous form the campaign has found.

## The correct mapping already exists in-tree

`nros-node/src/lifecycle_services.rs:65-75` maps the ids correctly and is used
on the **wire** path only. So `ros2 lifecycle set` behaves correctly while the
C++ API does not — the two disagree about the same node.

## Sibling

`LifecycleNode::shutdown()` (`lifecycle.hpp:164`) hardcodes id `5` =
`ShutdownUnconfigured`, whose only legal source state is `Unconfigured`, so it
cannot shut down an Inactive or Active node. Our own Rust state machine
(`nros-node/src/lifecycle.rs:137-151`) resolves the shutdown transition from
the current state; the C++ header bypasses it.

## Why nothing caught it

The row is unledgered, and it could not have been caught by the gate:
`scripts/api_parity/correlate.py:compare()` never compares return types or
struct field VALUES — when arities intersect and no substitution rule fires it
assigns `"same"` unconditionally (`:348`). An enum whose discriminants differ is
invisible to it by construction.

`lifecycle.json`'s `rust:LifecyclePollingNode::trigger_transition` also
cross-references a `cpp:` key that does not exist.

## Fix

Route `trigger_transition` through `lifecycle_services.rs`'s mapping so the C++
API and the wire agree, and resolve `shutdown()` from the current state. Then
decide whether our enum's discriminants should simply become upstream's —
nothing in a fixed arena requires a different numbering, so this looks like a
preference recorded as a divergence (RFC-0036).

## Resolution

Fixed 2026-09-06 by `0747a4c34`, by renumbering rather than translating: the
core enum takes `lifecycle_msgs/msg/Transition` ids, because the C side carried
the same latent defect and a boundary translation would have left
`NROS_LIFECYCLE_TRANSITION_ACTIVATE` and `trigger_transition(2)` meaning
different things inside one image. `c1db3a752` (2026-09-10) discharged the
prose-ratchet rows that cited this issue as in flight. The file itself was
never archived — the ledger row `cpp:LifecycleNode::trigger_transition` has said
"issue 1099 archived" since the fix; it is now true. Re-verified on `main`
2026-09-11:

* Ids are upstream's: `packages/core/nros-core/src/lifecycle.rs:92-101`
  (`Cleanup = 2`, `Activate = 3`, `Deactivate = 4`); `ErrorRecovery = 60`
  (`TRANSITION_ON_ERROR_SUCCESS`, `:114`), not upstream's 8 (DESTROY, which we
  do not implement); `from_u8` rejects the rest (`:44-51`).
* `shutdown()` resolves from the current state through
  `shutdown_transition_for(LifecycleState)`
  (`packages/api/nros-cpp/include/nros/lifecycle.hpp:105`).
* Pinned: `packages/api/nros-cpp/tests/compile/lifecycle_transition_ids.cpp`
  static-asserts the C literals against UPSTREAM values, run by `check-cpp`
  (`just/check/lanes.just:646`); nros-core's lifecycle tests pass (15/15).
* The dangling cross-reference is gone: `cpp:LifecycleNode::trigger_transition`
  exists in `lifecycle.json`, and no `.config` baseline row cites 1099.

State ids stay a deliberate divergence (`ErrorProcessing = 5` vs upstream 15;
5 is unassigned upstream, so it cannot silently mean another state) — recorded
with a test by the same commit.
