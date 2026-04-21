# Phase 86: `nros-lifecycle-msgs` Codegen Crate + Lifecycle Services

**Goal**: Add a `lifecycle_msgs`-compatible codegen crate so the C and
Rust APIs can register the REP-2002 lifecycle services (`~/change_state`,
`~/get_state`, `~/get_available_states`, `~/get_available_transitions`,
`~/get_transition_graph`) on a node, matching what `rclcpp_lifecycle` /
`rclc_lifecycle` do for upstream ROS 2.

**Status**: Not Started
**Priority**: Medium — the state machine already exists in
`nros-node::lifecycle`; this phase is about surfacing it to ROS 2 tooling
(`ros2 lifecycle set`, `ros2 lifecycle get`, `ros2 lifecycle nodes`).
**Depends on**: Phase 84 (Group B lifecycle state machine work — already
landed if/when the phase-84 branch is merged).

## Overview

Phase 84.B4 migrated the C API's lifecycle state machine to
`nros_node::lifecycle::LifecyclePollingNodeCtx` and made the C handle
opaque. That work left one piece open: the C (and Rust) APIs still can't
*expose* the lifecycle state machine to ROS 2 tooling because the
`lifecycle_msgs/{srv,msg}` types are not present in the codebase. This
phase adds them, plus the service-registration plumbing that reads /
drives `LifecyclePollingNodeCtx` from those service handlers.

### Upstream service surface (REP-2002)

| Service                                   | Request                 | Response                                |
|-------------------------------------------|-------------------------|-----------------------------------------|
| `~/change_state`                          | `Transition transition` | `bool success`                          |
| `~/get_state`                             | (empty)                 | `State current_state`                   |
| `~/get_available_states`                  | (empty)                 | `State[] available_states`              |
| `~/get_available_transitions`             | (empty)                 | `TransitionDescription[] transitions`   |
| `~/get_transition_graph`                  | (empty)                 | `TransitionDescription[] graph`         |

Upstream message types (`lifecycle_msgs/msg/`): `State`, `Transition`,
`TransitionDescription`, `TransitionEvent`.

## Work Items

- [x] 86.1 — Create `packages/interfaces/lifecycle-msgs/` following the
      `rcl-interfaces` pattern: `Cargo.toml` pointing at
      `nros-core` / `nros-serdes`, `package.xml` depending on upstream
      `lifecycle_msgs`, `generated/` directory populated by
      `cargo nano-ros generate`. Gate on a `ros-humble` / `ros-iron`
      feature axis (same as `rcl-interfaces`).
- [x] 86.2 — Add `pub mod lifecycle_services;` to `nros-node` behind a
      new `lifecycle-services` Cargo feature (mirroring the existing
      `param-services` layout). Module exposes:
      - `LifecycleServiceServers` — the 5 service servers.
      - `register_lifecycle_services(executor, node, state_machine)` on
        `Executor` — creates all 5 service handles and stashes a
        `LifecycleState { state_machine, services }` next to `params`.
      - Handler functions (`handle_change_state`, `handle_get_state`, …)
        that read / mutate the `LifecyclePollingNodeCtx`.
- [x] 86.3 — Wire the 5 services into `Executor::spin_once` so incoming
      requests drain during the normal poll cycle (same pattern as
      `ParamState`).
- [x] 86.4 — Add C FFI in `nros-c/src/lifecycle.rs`:
      - `nros_executor_register_lifecycle_services(exec, sm)` — takes
        the existing `nros_lifecycle_state_machine_t*` (post-Phase 84
        opaque handle) and registers the 5 services.
      - Gate on a new `lifecycle-services` Cargo feature on `nros-c` that
        forwards to `nros-node/lifecycle-services` and implies `alloc`.
- [x] 86.5 — Reference example: extend
      `examples/native/rust/zenoh/lifecycle-node/` (new) to demonstrate
      `ros2 lifecycle set /<node> configure` / `ros2 lifecycle get /<node>`
      driving the state machine.
- [x] 86.6 — Doc updates: book's `reference/rust-api.md` lifecycle
      section + `reference/c-api.md` to point at the new registration
      functions; `porting/custom-rmw.md` if the message encoding exposes
      anything backend-specific (unlikely — these are plain CDR).
- [x] 86.7 — Serde round-trip tests for every generated msg/srv in
      `nros-lifecycle-msgs`. Catches codegen drift (field re-ordering,
      missing variants) without needing a transport. Implemented in
      `nros-node::lifecycle_services::tests` (not in the generated
      crate, so regeneration can't clobber them): 11 round-trip tests
      covering `State`, `Transition`, `TransitionDescription`,
      `TransitionEvent`, and every service Request/Response pair.
- [x] 86.8 — Integration tests for `Executor::register_lifecycle_services`
      using `MockSession`. Covers: (a) registration succeeds and
      `lifecycle_state_machine_mut()` returns `Some`; (b) `spin_once`
      drains the (empty) service set without error; (c) the handler
      functions respond correctly when invoked against the
      executor-owned state machine through the accessor. Also walks
      the full Unconfigured → Inactive → Active → Inactive →
      Unconfigured cycle through registered `extern "C"` callbacks.
      Loadable-mock extensions to `MockServiceServer` (simulating a
      live ChangeState request) remain deferred.

## Design Notes

- **Where `LifecyclePollingNodeCtx` lives**: after Phase 84.B4 the C
  wrapper stores it inline in the opaque `_opaque_storage` field of
  `nros_lifecycle_state_machine_t`. The new
  `nros_executor_register_lifecycle_services` takes `*mut
  nros_lifecycle_state_machine_t` and passes a raw pointer to the
  inner `LifecyclePollingNodeCtx` into `Executor::register_lifecycle_services`.
  That keeps a single state machine authoritative for both direct
  callback use and ROS 2 service use.
- **Thread / reentrancy story**: `register_lifecycle_services` requires
  `&mut` access to both the executor and the state machine. Because the
  state machine lives outside the executor's arena, the borrow-check is
  a split-borrow at call sites (same trick `ParamState::process` already
  uses for the parameter server).
- **Event-side publisher (`~/transition_event`)**: out of scope for
  86.1–86.6. Adding a publisher that emits a `TransitionEvent` on every
  transition is a small follow-up once the services themselves are
  landed.
- **`lifecycle_msgs` service hashes**: need to match upstream ROS 2 so
  `rmw_zenoh` routes correctly. Codegen already computes these — no
  manual type hash maintenance needed.

## Acceptance Criteria

- [ ] `ros2 lifecycle nodes` lists an nros test node.
- [ ] `ros2 lifecycle get /<node>` returns the current state (string +
      id) and round-trips correctly after a
      `nros_lifecycle_change_state` call.
- [ ] `ros2 lifecycle set /<node> configure` drives
      `LifecyclePollingNodeCtx` through `Configure`, runs the user's
      callback, and reflects the new state on the next
      `ros2 lifecycle get`.
- [ ] `ros2 lifecycle list /<node>` prints the 5 expected transitions
      reachable from the current state.
- [ ] No `static mut` added in the service registration path —
      everything lives inside the executor's `Box<LifecycleState>`,
      matching `Box<ParamState>`.

## Notes

- Naming mirrors upstream: crate `nros-lifecycle-msgs` on publish, directory
  `packages/interfaces/lifecycle-msgs/` for consistency with
  `rcl-interfaces` (no `nros-` prefix on the directory path since it's a
  generated mirror of an upstream ROS 2 package).
- Keep the `lifecycle-services` feature off by default — users who
  don't need ROS 2 tooling integration shouldn't pay for the 5 service
  servers and their buffers (same argument that applies to
  `param-services`).
- This phase does *not* change the state machine itself. If additional
  transitions or introspection (like `get_transition_graph` needing a
  list of all transitions regardless of current state) require new
  helpers on `LifecyclePollingNodeCtx`, add them as small pure-Rust
  additions in 86.2 rather than letting handler code duplicate the
  transition table.
