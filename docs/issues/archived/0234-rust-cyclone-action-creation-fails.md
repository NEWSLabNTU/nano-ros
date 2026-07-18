---
id: 234
title: "native rust CycloneDDS action server/client fail at creation (ActionCreationFailed) — the typed-action-descriptor path has no pure-rust equivalent"
status: resolved
type: bug
severity: medium
area: rmw
related: [issue-0233, issue-0067, issue-0068]
resolved_in: "fix(#234): native rust cyclonedds action delivers"
---

## Resolution (2026-07-18)

RESOLVED — the native rust cyclone action pair delivers the order-10 Fibonacci
result (`Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`); server sees
`Received goal request with order 10`, executes, succeeds. Matrix cell
`Native Rust Cyclonedds Action` flipped to `Runtime`
(`test_native_cyclonedds_rust_action`, fixtures `action-{server,client}`
target-cyclonedds).

The original hypothesis (a missing *descriptor-provisioning* path) was only
half of it. Two independent root causes, both surfaced with runtime tracing +
a registry dump:

1. **The action-protocol descriptors were never registered on the path the
   example uses.** `RosAction::register_protocol_types` (which registers the
   `action_msgs` types the cancel service + status publisher serialize —
   `CancelGoal_{Request,Response}`, `GoalStatusArray`) was (a) gated behind the
   generated msg crate's own `rmw-cyclonedds` feature + a named
   `nros_rmw_cyclonedds::register::<M>()` call, which the standard example build
   never turned on, so it compiled to a no-op; and (b) **not even called** from
   `nros-node`'s `node.rs` typed action creators (`create_action_server_sized`
   / `create_action_client_sized`) — the ones the direct-executor
   `create_action_server::<A>` example materialises through. (The callback
   `executor/action.rs` path did call it.) Fix: the codegen now routes
   `register_protocol_types` through the generic
   `nros_rmw::register_type_descriptor` seam (no cfg gate, no named-backend dep
   — same seam the 8 action envelopes use via `register_type::<M>()`), and both
   `node.rs` typed paths now invoke it before creating the cancel / status
   entities.

2. **The Cyclone backend doubled the per-channel wrapper infix.**
   `descriptors_for_service` → `action_effective_base` (send_goal / get_result)
   and `action_topic_type` (feedback) were written assuming the caller passes
   the BARE action type `<A>_` and append `_SendGoal_` / `_GetResult_` /
   `_FeedbackMessage_` themselves. The typed Rust paths instead advertise the
   ALREADY-per-channel type (`<A>_SendGoal_` — what a real `rcl_action` peer
   matches on), so the append produced `<A>_SendGoal_SendGoal_Request_` /
   `<A>_FeedbackMessage_FeedbackMessage_`, which resolved no descriptor →
   `NROS_RMW_RET_UNSUPPORTED` → `ActionCreationFailed`. Fix: both mappings are
   now idempotent — if the wrapper suffix is already present, the type passes
   through unchanged (the raw / C / C++ path still passes the bare type and
   appends as before).

Both fixes are required together: (1) provisions the CancelGoal / GoalStatus
descriptors; (2) lets the SendGoal / GetResult / FeedbackMessage lookups
resolve.

Files: `packages/cli/rosidl-codegen/templates/action_nros.rs.jinja`,
`packages/cli/rosidl-codegen/templates/cargo_nros.toml.jinja`,
`packages/cli/rosidl-codegen/src/templates.rs`,
`packages/cli/rosidl-bindgen/src/generator.rs` (codegen: generic seam + drop the
`rmw-cyclonedds` feature / named dep, add `nros-rmw` dep);
`packages/core/nros-node/src/executor/node.rs` (call `register_protocol_types`
in both typed action paths);
`packages/dds/nros-rmw-cyclonedds/src/service.cpp` +
`packages/dds/nros-rmw-cyclonedds/src/descriptors.cpp` (idempotent infix).

Note (latent, not blocking): the runtime registry also stores garbage slash-form
type-name keys because the Rust registry passes the non-NUL-terminated
`nros_serdes::Message::TYPE_NAME` `&'static str` to the C++
`register_descriptor` (which reads to NUL). The mangled `m_typename` keys — the
ones every lookup uses — are clean, so resolution is unaffected; worth a
follow-up NUL-terminate.

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
