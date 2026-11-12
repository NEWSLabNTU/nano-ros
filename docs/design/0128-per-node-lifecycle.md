---
rfc: 0128
title: "A managed node has its own REP-2002 state machine, served under its own name, as stock ROS 2 does"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: []
amends: [RFC-0036, RFC-0037, RFC-0060]
supersedes: []
superseded-by: null
---

# RFC-0128 — Per-node lifecycle

**Status:** Draft (2026-11-12). Written before any code. No phase yet; the
delivery order is at the end. Closes the design question of
[issue 2428](../issues/2428-workspace-lifecycle-is-one-machine-per-image.md)
and decides [issue 2429](../issues/2429-c-executor-lifecycle-services-empty-node-name.md).

## Summary

User direction (2026-11-12): "I'd like to ensure the interface compatibility
among nano-ros and stock ROS 2." For lifecycle that is option A of issue 2428:
REP-2002 the way stock ROS 2 does it.

- **One state machine per managed node.** It is served under the node's own
  name: `/<node>/change_state`, `get_state`, `get_available_states`,
  `get_available_transitions` and `get_transition_graph`, plus the
  `/<node>/transition_event` topic. No `/nros_app`.
- **Transition callbacks** `on_configure`, `on_activate`, `on_deactivate`,
  `on_cleanup`, `on_shutdown` and `on_error`, in Rust, C and C++. They return
  SUCCESS, FAILURE or ERROR, with rclcpp's defaults.
- **The current state is readable** from the callback context.
- **A managed publisher** drops while its node is not active, as rclcpp's
  `LifecyclePublisher` does.
- **`autostart` per node,** defaulting to the image's `[lifecycle] autostart`.
- **Five queryables and one publisher per managed node,** counted by the
  derived pools. An image with no managed node pays nothing.

Acceptance is behavioural: `ros2 lifecycle nodes`, `get`, `set` and `list`
against a board answer exactly as they do against an rclcpp `LifecycleNode`,
and a managed node stays silent until it is activated.

## Motivation

Issue 2428 measured a workspace image with `[lifecycle] autostart = "none"`:

```text
$ ros2 lifecycle nodes
/nros_app
$ ros2 lifecycle get /sensor_node
Node not found
```

Meanwhile `sensor_node` published from boot. Three defects, each a place
where a ROS 2 user's expectation fails:

1. **The machine belongs to the image, not the node.** The executor holds one
   `lifecycle: Option<Box<LifecycleRuntimeState>>`
   (`packages/core/nros-node/src/executor/spin.rs`). `register_lifecycle_services_named`
   assigns it with no guard, so a second registration silently replaces the
   first and drops its servers. Two C++ `LifecycleNode`s on one executor
   overwrite each other's machine for the same reason.
2. **The machine is named after the executor's boot identity.** Every board
   entry passes `"nros_app"` as the compiled default node name, and
   `register_lifecycle_services()` names the services after it. On the C road
   that name is empty, and the services come out as `//get_state`
   (issue 2429).
3. **No node can see or follow its state.** A Rust component has
   `Component::register` and `ExecutableNode::on_callback`, and neither offers
   the state or a transition hook. `LifecycleCallbacks` exists only at the
   `Executor` level, which the generated entry owns. A C component
   (`RCLC_C_COMPONENT`) has no lifecycle hook at all. Nothing gates a
   publisher.

What already works, and is kept:

- the five services and `transition_event` (issue 1587);
- transition IDs equal to `lifecycle_msgs` (issue 1099);
- `shutdown` from all three primary states;
- the error-recovery transition (60);
- `register_lifecycle_services_on(NodeId)` (phase-488, Nav2), which already
  names the services after a node record rather than the executor.

The transport is fine: issue 2428's services round-trip. The fault is the
cardinality and the binding.

## Design

### D1. One machine per managed node, in a table sized at build time

The executor's single `lifecycle` slot becomes a table of
`LifecycleRuntimeState`, keyed by the owning `NodeId`. Its capacity is
`MAX_LIFECYCLE_NODES`, derived from the image's managed-node count (D7) and
zero when there are none.

- `register_lifecycle_services_on(NodeId)` adds an entry.
- Registering a node that already has a machine is an error
  (`AlreadyRegistered`), never a replacement.
- A full table is an error naming the knob, never a silent drop.
- Each machine has its own state, its own callbacks and context (D4), and its
  own announcement queue for `transition_event`.

The accessors take the node: `lifecycle_state_machine(NodeId)`.

Every message this RFC adds that a user can see names the node, the knob or
the transition, and carries no issue, phase or RFC id. That covers runtime
refusals, the dropped-publish log line (D3), build refusals and generated
comments, per the user's ruling of 2026-11-12. The ids go in source comments.

### D2. A managed node is one the model or its component says is managed

The fact already exists: `structure.nodes[].lifecycle: bool` in the
SystemModel (RFC-0060, "The lifecycle fact has the right shape already"). Its
inputs are the launch file's `<lifecycle_node>` / `LifecycleNode(...)` and a
contract's `lifecycle: true`.

This RFC adds a third input: **the component's own declaration.** In stock
ROS 2 the class decides. A node whose class derives from
`rclcpp_lifecycle::LifecycleNode` is managed however it is launched, and
`LifecycleNode` in a launch file only says that launch expects it to be. So:

- **Rust:** a component implementing `LifecycleComponent` (D4) is managed.
- **C++:** a component deriving from `rclcpp_lifecycle::LifecycleNode` is
  managed. This already holds in code.
- **C:** a component that makes its node a lifecycle node (D4) is managed.
- **Launch as `<lifecycle_node>` with an unmanaged component:** stock ROS 2
  would wait for services that never appear. Here the node gets a machine
  whose callbacks all take the defaults, so it can be driven, and its
  publishers are gated (D3). **Open question Q1.**

The fact reaches the runtime by ONE route: the entry registers a machine for
exactly the nodes the resolved model marks managed. The component-side
declaration is folded into the model by the probe (the metadata sidecar
already reports per-component facts), so `nros::main!` and C/C++ codegen
read one list.

### D3. A managed node's publishers follow its state

Every publisher a managed node creates is a managed publisher:

- **Activation:** it is activated when its node enters `active` and
  deactivated when it leaves.
- **While inactive:** `publish` sends nothing and returns success. The first
  dropped message logs once, as rclcpp's `LifecyclePublisher` warns, through
  `nros_log`.

Subscriptions, services, timers and clients are not gated, because rclcpp
gates none of them. A node that should go quiet in `inactive` does so in its
callbacks, the same as on Linux. The silent-until-active cell (D9) needs only
publisher gating.

This needs no callback API, so it ships in phase B with D1, D2, D6 and D7. In
that phase a node's machine has default callbacks, and gating is the
behaviour a user can see.

**Verification before Stable:** whether rclcpp activates managed entities
automatically. If it does (and from which distro), nano-ros matches. If it
does not on Humble (the user calls `publisher->on_activate()` in
`on_activate`), see **Q2**.

### D4. Transition callbacks in each language

The signatures and defaults follow rclcpp. Each callback receives the
previous state and returns `CallbackReturn::{Success, Failure, Error}`. The
defaults are Success for configure, activate, deactivate, cleanup and
shutdown, and Failure for `on_error`.

- **Rust** (no stock API to match, since rclrs has no lifecycle node): a
  `LifecycleComponent` trait beside `Component`. Its methods have default
  bodies, and each takes `(&mut self, previous: &State, ctx: &mut
  TransitionCtx)`. `TransitionCtx` gives what `NodeContext` gives a component
  at runtime: its node, its publishers and its parameters. The existing
  `LifecycleCallbacks` trait stays as the executor-level form for a
  hand-written entry.
- **C:** upstream `rclc_lifecycle`'s API, since RFC-0037 makes the C surface
  rclc and only rclc. That is `rclc_lifecycle_node_t`,
  `rclc_make_node_a_lifecycle_node(...)`, `rclc_lifecycle_register_on_*`, and
  the service-server initialiser upstream names. A C component calls it from
  its configure function on its own node, which is how a stock rclc program
  does it. The executor-scoped family
  (`rclc_executor_register_lifecycle_services(exec)` and its two siblings)
  retires with D8. The exact Humble and Jazzy spellings are to be taken from
  upstream's headers, not from this text. **Verification before Stable.**
- **C++:** `rclcpp_lifecycle::LifecycleNode` keeps its virtual `on_*` hooks.
  Its `start()` registers its OWN node's machine (`_for_node`), which now adds
  a table entry instead of overwriting one. `LifecyclePublisher` stays, and
  every publisher a `LifecycleNode` creates is one (D3).

### D5. The state machine conforms where a callback can observe it

Stock behaviour, which nano-ros adopts:

- **`on_error` runs automatically.** A callback returning ERROR moves the node
  to `errorprocessing` and runs `on_error` at once. Its SUCCESS goes to
  `unconfigured` and its FAILURE to `finalized`. Today `on_error` runs only on
  an explicit transition 60, and FAILURE stays in `errorprocessing`.
- **The state during a callback is the transition state.** `configuring` (10)
  through `errorprocessing` (15): `get_current_state()` inside `on_configure`
  reports `configuring`, as rclcpp does. The state accessor (D1) and
  `TransitionCtx` both read it.
- **`transition_event` carries what rclcpp publishes for the same transition.**
  Whether that is one event (start to goal) or one per hop through the
  transition state is to be measured against rclcpp. **Verification before
  Stable.**

Still not implemented, as before:

- **CREATE (0) and DESTROY (8).** They are not reachable through
  `change_state` on rclcpp either.
- **A multi-threaded transition.** Callbacks run on the node's executor
  thread, so `get_state` from outside never sees a transition state. rclcpp
  can show one under a multi-threaded executor. The wire difference is
  timing, not shape.

### D6. `autostart` per node, with the image value as the default

`structure.nodes[].lifecycle_autostart` (`None | Configure | Active`) is set
per node from the launch file's `autostart`. A node that leaves it unset takes
the image's `[lifecycle] autostart`, which is what RFC-0060 already states
("applied by nano-ros only to lifecycle nodes whose own `lifecycle_autostart`
the model left unset").

The code diverged from that text: `reduce_lifecycle_autostart`
(`nros-orchestration-ir/src/overlay.rs`, phase-486 W0) builds one value per
image and errors when nodes disagree, because "the runtime has one
executor-wide boot transition". D1 removes that reason, so the reducer goes.
The entry runs each managed node's autostart at boot, in launch declaration
order, before the spin loop. This is what `launch_ros` does by service calls
after spawning the node.

### D7. Five queryables and one publisher per managed node, derived

Every count of the lifecycle family becomes per managed node:

- `InfraServices::queryables`, `+1` publisher per managed node;
- `nros_sizing_descriptor::infra`;
- `nros-zpico-build`'s runner;
- `executor_sizing::LIFECYCLE_SERVICE_SLOTS`;
- the static inbox rings.

Today each counts the family once per executor ("deliberately has no twin"
beside `param_service_slots(nodes)`). Each becomes `5 × managed_nodes`
queryables and `1 × managed_nodes` publishers, and `MAX_LIFECYCLE_NODES` is
derived from the same list as D2.

`knobs.toml`'s help for `ZENOH_MAX_QUERYABLES` already says "five per node"
and becomes true. An image with no managed node pays zero, where today any
image with a `[lifecycle]` block pays five.

### D8. Services are only ever named after a declared node

The executor-identity form `register_lifecycle_services()` retires from every
generated entry. The C executor-scoped family goes with it (D4).

`register_lifecycle_services_named` refuses an empty node name, which closes
issue 2429's `//get_state`. `/nros_app` stops appearing in `ros2 lifecycle
nodes` because nothing registers a machine for it. Whether it should appear in
`ros2 node list` at all is RFC-0045's question, not this one.

### D9. Acceptance is a diff against rclcpp, per RMW

The reference is a stock rclcpp `LifecycleNode` on the host, the ROS 2 demos'
`lifecycle_talker`. Each cell runs the same `ros2 lifecycle` commands against
it and against a board image, and compares the outputs.

- **`nodes`** lists exactly the managed nodes: no `/nros_app`, no unmanaged
  node.
- **`get /<node>`** is the right state after each step of configure, activate,
  deactivate, cleanup and shutdown.
- **`list /<node>`** gives the same transitions, IDs and labels as the
  reference in every primary state.
- **`set /<node> <t>`:** `Transitioning successful` for a legal transition,
  and the reference's refusal for an illegal one.
- **Two managed nodes in one image** have independent states.
- **Silent until active:** a managed talker with `autostart = "none"`
  publishes nothing on its topic until `ros2 lifecycle set /<node> activate`,
  stops at `deactivate`, and resumes at the next `activate`. This is the
  grading line of the issue 2269 lifecycle lab.

The cells are added to `interop::CELLS`, zenoh first, then each RMW the
interop matrix carries, and in each language as its D4 surface lands. The
existing `workspace-zephyr-rs-lifecycle` cell, which asserts only that the
services answer, gains the silence assertion.

## Delivery

**Phase B: per-node machines and runtime gating.** This unblocks the issue
2269 lab.

- D1, D2 (model and launch inputs; the component declaration where it already
  exists: C++), D3, D6, D7 and D8.
- The issue 2429 refusal.
- D9's cells: `nodes`, `get`, `set` and `list`, two nodes, silent until
  active.

At the end of phase B every managed node has default callbacks, and
`ros2 lifecycle` behaves as against an rclcpp node whose callbacks all
succeed.

**Phase C: callbacks.**

- D4 in Rust, C and C++.
- The component declaration as a D2 input for Rust and C.
- D5's conformance and the state accessor in the callback context.
- D9's cells per language, with a callback returning FAILURE and ERROR.

A phase number is reserved when this RFC is accepted.

## Amendments

- **RFC-0036.** "No lifecycle-node graph — a simplified state model for
  embedded executors" narrows to D5's two remaining differences: no
  CREATE/DESTROY, and single-threaded transitions.
- **RFC-0037.** The C lifecycle surface is upstream `rclc_lifecycle`'s (D4).
  The `rclc_executor_*lifecycle*` family leaves the inventory.
- **RFC-0060.** The text on per-node `lifecycle_autostart` stands. Its
  phase-486 W0 reducer, which enforced one value per image, is retired (D6).

## Open questions

- **Q1. A plain component launched as `<lifecycle_node>`.** Recommended: it is
  managed, with default callbacks and gated publishers (D2). That lets the
  phase-B lab manage a talker that has no callbacks. Alternative: refuse it at
  build time, naming the component, as stock ROS 2 effectively does by
  hanging.
- **Q2. If rclcpp on a supported distro does NOT activate managed publishers
  by itself.** Recommended: nano-ros activates them on entering `active`
  anyway (D3). A user who writes `publisher->on_activate()` in `on_activate`,
  as Humble requires, gets a harmless no-op, and the lab needs no callback.
  Alternative: match each distro exactly, which ties phase B to phase C.

## Verification before Stable

These are claims about stock behaviour, read from rclcpp and rclc sources on
Humble and Jazzy rather than from memory:

1. whether rclcpp auto-activates managed entities, and from which distro (D3,
   Q2);
2. upstream `rclc_lifecycle` spellings on Humble and Jazzy (D4);
3. `transition_event`'s event count per transition (D5).
