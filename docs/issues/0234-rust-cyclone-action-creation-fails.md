---
id: 234
title: "native rust CycloneDDS action server/client fail at creation (ActionCreationFailed) — the typed-action-descriptor path has no pure-rust equivalent"
status: open
type: bug
severity: medium
area: rmw
related: [issue-0233, issue-0067, issue-0068]
---

## Finding (issue #233 cell 1, 2026-07-18)

Building `examples/native/rust/action-{server,client}` with
`--no-default-features --features rmw-cyclonedds` and running the pair on a
shared ROS domain, BOTH sides fail at creation — deterministically, before
any goal is sent:

```
[ERROR action_server] Failed to create action server: ActionCreationFailed
thread 'main' panicked at src/main.rs:38:10:
Failed to create action server: ActionCreationFailed
...
[ERROR action_client] Failed to create action client
```

The rust cyclone **pub/sub** pair and (issue #233) the rust cyclone
**service** pair both deliver — only ACTION creation fails. The C and C++
cyclone action pairs work (they are `Runtime` cells, e.g.
`test_native_cyclonedds_action`, `test_threadx_linux_cyclonedds_action`).

## Root cause (hypothesis to confirm)

Cyclone needs a `dds_topic_descriptor_t` per message type, looked up by
mangled name via the backend's `find_descriptor`. The C/C++ path compiles
the per-type descriptor TUs from `descriptors.cpp`
(`nros-rmw-cyclonedds`); the pure-rust path relies on the
`nros/rmw-cyclonedds` marker (issue #67) which wires pub/sub + service type
creation but does NOT emit the ACTION-type descriptors (the goal/result/
feedback service+topic set — action_msgs, plus the per-action typed
messages). So `create_action_static::<Act>` can't resolve its descriptors
and returns `ActionCreationFailed`.

Compare with issue #68 (action goal_id wire layout) and #67 (typed cyclone
needs the marker) — this is the action-specific extension of the same
descriptor-provisioning gap.

## Fix direction

Make the rust cyclone action path acquire the same action-type descriptors
the C/C++ `descriptors.cpp` provides — either by generating the rust-side
descriptor registration for action types under the `rmw-cyclonedds`
feature, or by linking the C descriptor TUs for the action message set into
the rust image (the pattern the rust cyclone pub/sub path already uses for
std_msgs). Once creation succeeds, flip the
`Native Rust Cyclonedds Action` matrix cell to `Runtime` with a
`test_native_cyclonedds_rust_action` lane and close the #233 sub-item.
