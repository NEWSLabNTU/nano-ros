---
rfc: 0128
title: "A managed node has its own REP-2002 state machine, served under its own name, as stock ROS 2 does"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-554]
amends: [RFC-0036, RFC-0037, RFC-0045, RFC-0060]
supersedes: []
superseded-by: null
---

# RFC-0128 — Per-node lifecycle

**Status:** Draft (2026-11-12). Every question is ruled. Written before any
code. Work: [phase-554](../roadmap/phase-554-per-node-lifecycle.md), whose
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
- **Cost that grows slowly with managed nodes.** On zenoh, one shared
  lifecycle queryable and dispatcher serve every managed node. Each node adds
  its liveliness tokens and its state, not five queryables and a publisher.
  On the DDS backends (Cyclone, XRCE) each node pays its own readers and
  writers. Both costs are measured. An image with no managed node pays
  nothing.

Acceptance is behavioural: `ros2 lifecycle nodes`, `get`, `set` and `list`
against a board answer exactly as they do against an rclcpp `LifecycleNode`,
and a managed node stays silent until it is activated.

## Rulings

The user ruled on this RFC on 2026-11-12, relayed by the manager session:

- "I'd like to ensure the interface compatibility among nano-ros and stock
  ROS 2." This is the direction the whole RFC follows.
- "Take your advice on lifecycle questions." Q1 and Q2 are decided as
  recommended (D2, D3), and so is the zenoh memory design (D7).
- On D8: "Hide the node as default." The image announces only its declared
  nodes, and `/nros_app` is no longer announced. A read-only container is an
  optional follow-up issue, not part of this RFC.
- D6: autostart transitions run on the executor's first spin pass, never
  before the spin loop.
- D7, the DDS backends: the per-node reader and writer cost is accepted,
  stated and measured.
- The RFC becomes Stable only after the three upstream verifications at the
  end.

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
  would wait for services that never appear. Here the node is managed: it
  gets a machine whose callbacks all take the defaults, so it can be driven,
  and its publishers are gated (D3). Ruled 2026-11-12 (Q1).

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

**On every distro,** nano-ros activates a managed node's publishers when the
node enters `active` (ruled 2026-11-12, Q2). Where rclcpp leaves that to the
user, the user's own `publisher->on_activate()` in `on_activate` is a
harmless no-op. Verification item 1 found that on Humble and on Jazzy rclcpp
activates only through the base `LifecycleNode::on_activate`. So nano-ros
does more than rclcpp only for an overriding class that forgets the base
call, and the book says so.

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
  retires with D8. The spellings and shapes are upstream's, read on
  2026-11-12 (verification item 2). Callbacks are `int (*)(void)`, where
  `RCL_RET_OK` means success and anything else means failure. They are
  registered per goal state, so `on_shutdown` and `on_error` go through
  `rclc_lifecycle_register_callback`. A callback reaches its own data the
  way a stock rclc callback does, through file-scope state.
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
- **`transition_event` carries what rclcpp publishes for the same transition**
  (verification item 3): two events per transition, primary → transition
  state and then transition state → result, and a third when a callback
  returns ERROR. Each event sets `start_state` and `goal_state` and leaves
  `transition` and `timestamp` empty, as rcl does.

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
The entry runs each managed node's autostart transitions on the executor's
**first spin pass,** in launch declaration order. That is after every node is
created and its services are up, and never before the spin loop (ruled
2026-11-12). This is what `launch_ros` does: it drives the transitions by
service calls once the node is running. A callback that publishes in
`on_activate` therefore runs with the transport live, as it would on Linux.

### D7. Cost per managed node: one shared zenoh queryable; per-node DDS entities

Ruled 2026-11-12. The goal is that the RAM cost of one more managed node
stays small on the smallest supported board, without anything a stock ROS 2
tool can see changing.

**zenoh.** One lifecycle queryable per image serves every managed node.

- **The queryable.** It is declared on a wildcard key that covers the five
  service keys of all the image's managed nodes. A dispatcher reads the node
  and the service from the request's key and routes the request to that
  node's machine (D1).
- **Its inbox** is sized for lifecycle requests, which are small and
  fixed-shape, not for the image's generic service size.
- **Responses.** `get_available_states`, `get_available_transitions` and
  `get_transition_graph` are built on demand from const tables, so no
  response is held in RAM.
- **What stays per node.** Each managed node keeps its own liveliness tokens:
  one per service and one advertising `/<node>/transition_event`. So
  `ros2 service list`, `ros2 topic list` and `ros2 lifecycle nodes` show
  exactly what they show for a stock node.
- **`transition_event`** goes out as an on-demand put on
  `/<node>/transition_event`. It is not a publisher held per node.

So a managed node adds its state, its tokens and its callback record. It adds
no queryable, publisher or service buffer.

**Cyclone and XRCE.** Each managed node pays its own five service reader and
writer pairs and its `transition_event` writer, because DDS discovery
announces endpoints, and a shared endpoint would show in `ros2 service list`
under the wrong name. The cost is accepted (ruled 2026-11-12), stated in the
book's lifecycle chapter, and measured (D9).

**Derived pools.** Every count of the lifecycle family becomes per managed
node:

- `InfraServices`;
- `nros_sizing_descriptor::infra`;
- `nros-zpico-build`'s runner;
- `executor_sizing::LIFECYCLE_SERVICE_SLOTS`;
- the static inbox rings.

Today each counts the family once per executor. On zenoh the queryable count
is `1` for any image with a managed node, and the token count is
`6 × managed_nodes`. On the DDS backends the counts are five service pairs
and one writer per managed node. `MAX_LIFECYCLE_NODES` comes from the same
list as D2. An image with no managed node pays zero, where today any image
with a `[lifecycle]` block pays five queryables. The help text of
`ZENOH_MAX_QUERYABLES` in `knobs.toml`, which says "five per node", is
corrected to match.

### D8. The image announces only its declared nodes

Ruled 2026-11-12: "Hide the node as default." The executor's own identity is a
session label, not a node. It is `"nros_app"` on the Rust embedded boards and
`"node"` on hosted Rust and every C/C++ runner, and is reached only when
nothing names the image. No backend announces that label as a node, and
everything that uses it today moves to a declared node or is dropped.

Which declared node: an entity created **without** a node is attributed to the
image's **primary node**, the first declared node in launch declaration order
(`NodeId::PRIMARY`, the rule executor-level parameters already follow). An
image with no declared node at all is a direct-API image: its executor
identity is its one node, announced under the name its config gives. That is
a name the user chose, so it is declared. With no name configured, such an
image is refused at open, naming `node_name`, rather than announcing the
default.

Each user of the identity today, and what happens to it:

| Today | Backend | Becomes |
|---|---|---|
| zenoh's "primary" node liveliness token, declared at session open and dropped only when another node's entity appears (`shim/session.rs`) | zenoh | **Dropped.** zenoh announces a node lazily, with its first entity, as Cyclone already does (issue 1269). |
| XRCE participant name and session-key input (`xrce/src/session.c`) | XRCE | **Kept, not a node.** It was never announced as one. It stays a label, still unique per client, because the Agent keys sessions on it. |
| The cffi fallback that puts an entity with no `node_name` under the session's name (`rmw/cffi/src/lib.rs` `node_slot`) | all | **Moves** to the primary node. |
| Executor-level registrations with `node_id == None` (`open_subscription`, `register_service*`, `register_service_client*`; the action fallback) | all | **Moves** to the primary node. |
| The six parameter services and `/parameter_events` for a zero-node image | all | **Kept** for direct-API images only, under the configured name (above). With nodes declared they are already per node. |
| Lifecycle services and `transition_event` (`register_lifecycle_services()`; the C/C++ generated entries; rclc's executor-scoped family) | all | **Moves** to each managed node (D1). The executor-identity form retires. |
| The contract `/diagnostics` reporter (`DiagSink`, a raw session publisher) | all | **Moves** to the primary node. In stock ROS 2 a node publishes diagnostics. |
| The node-table spare slot for "the session's own name" (`entity_inventory.rs`, issue 1740), held for the reporter and lifecycle | sizing | **Dropped.** Both now live on declared nodes. |
| rclc's empty executor identity (issue 2429) and its legacy node without a node record | C | **Moves.** An rclc node gets a node record at init, so its entities name it. An empty name is refused. |
| Remap resolution against the current identity | all | **Moves** to the entity's node. |

Not affected:

- `/rosout`, which a user creates on their own node;
- `/clock`, which already waits for the primary node;
- the boot-config tests, which test the rung, not the graph.

Two e2e tests move:

- `interop_e2e`'s `/lifecycle_demo` was the executor identity named by
  config. Its example declares the node instead.
- Entry tests that take the first name from `ros2 lifecycle nodes` keep
  working, now with a declared node's name.

A **read-only component container**, so that `ros2 component list` answers
like a stock one, is an optional follow-up:
[issue 2474](../issues/2474-readonly-component-container.md), not part of this
RFC.

### D9. Acceptance is a diff against rclcpp, per RMW

The reference is a stock rclcpp `LifecycleNode` on the host, the ROS 2 demos'
`lifecycle_talker`. Each cell runs the same `ros2 lifecycle` commands against
it and against a board image, and compares the outputs.

- **`nodes`** lists exactly the managed nodes: no `/nros_app`, no unmanaged
  node.
- **`ros2 node list`** shows exactly the image's declared nodes, on every
  backend, from boot onward. The executor identity never appears (D8).
- **`get /<node>`** is the right state after each step of configure, activate,
  deactivate, cleanup and shutdown.
- **`list /<node>`** gives the same transitions, IDs and labels as the
  reference in every primary state.
- **`set /<node> <t>`:** `Transitioning successful` for a legal transition,
  and the reference's refusal for an illegal one.
- **Two managed nodes in one image** have independent states.
- **RAM per additional managed node,** measured with `just mem-report` on the
  smallest supported board, for zenoh and for each DDS backend. The image
  with N+1 managed nodes is compared against the image with N.
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

- D8 in full: no backend announces the executor identity, and every user of
  it moves per the D8 table.
- D1; D2 (model and launch inputs, plus the component declaration where it
  already exists, which is C++); D3; D6; D7 (the zenoh shared queryable and
  dispatcher, and the DDS per-node entities).
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

The work items and their acceptance are in
[phase-554](../roadmap/phase-554-per-node-lifecycle.md).

## Amendments

- **RFC-0036.** "No lifecycle-node graph — a simplified state model for
  embedded executors" narrows to D5's two remaining differences: no
  CREATE/DESTROY, and single-threaded transitions.
- **RFC-0037.** The C lifecycle surface is upstream `rclc_lifecycle`'s (D4).
  The `rclc_executor_*lifecycle*` family leaves the inventory.
- **RFC-0045.** The board compiled-default rung (`"nros_app"`) names the
  session, not a graph node (D8).
- **RFC-0060.** The text on per-node `lifecycle_autostart` stands. Its
  phase-486 W0 reducer, which enforced one value per image, is retired (D6).

## Open questions

None. Q1 (a plain component launched as `<lifecycle_node>` is managed) and Q2
(publishers are activated on entering `active` on every distro) were ruled on
2026-11-12, as were D6 and D7.

## Verification before Stable

The RFC becomes Stable only after these three checks (ruled 2026-11-12). Each
is a claim about stock behaviour. All three were read from upstream source on
2026-11-12 (phase-554 W0), at the `humble` and `jazzy` branch tips:

| Repo | humble | jazzy |
|---|---|---|
| ros2/rclcpp | `b54525b36c4a` | `35b821a9fb1f` |
| ros2/rcl | `b50489adc465` | `18d96c9c2b98` |
| ros2/rclc | `10eadcc6444a` | `45ef3de56758` |

On all three points Humble and Jazzy behave the same.

1. **Does rclcpp activate managed entities by itself?** Only through the base
   class's default callback. `LifecycleNode::on_activate(const State &)`
   calls `impl_->on_activate()`, which calls `on_activate()` on every managed
   entity, and `on_deactivate` mirrors it
   (`rclcpp_lifecycle/src/lifecycle_node.cpp`, `on_activate`). A user class
   that overrides `on_activate` and does not call the base leaves its
   publishers inactive until it calls `publisher->on_activate()` itself, on
   both distros.
   - **Consequence for D3:** nano-ros activates on entering `active` whatever
     the callback does (Q2). So nano-ros does more than rclcpp for an
     overriding class that forgets the base call, and the same for every
     other class.
   - **Timers:** Jazzy's `LifecycleNodeInterfaceImpl` records timers in
     `weak_timers_`, but nothing reads that list to gate them. Timers are not
     managed on either distro, which matches D3.
2. **Upstream `rclc_lifecycle` (D4).** The header is byte-identical on Humble
   and Jazzy:
   - **Making a lifecycle node:** `rclc_make_node_a_lifecycle_node(rclc_lifecycle_node_t *,
     rcl_node_t *, rcl_lifecycle_state_machine_t *, rcl_allocator_t *, bool
     enable_communication_interface)`.
   - **Changing state:** `rclc_lifecycle_change_state(node, transition_id,
     publish_update)`.
   - **Registering callbacks:** `rclc_lifecycle_register_on_configure`,
     `_on_activate`, `_on_deactivate` and `_on_cleanup`, plus the generic
     `rclc_lifecycle_register_callback(node, goal_state, cb)`.
   - **Service servers:** `rclc_lifecycle_init_get_state_server`,
     `_get_available_states_server` and `_change_state_server`, each taking an
     `rclc_lifecycle_service_context_t *` and an executor.
   - **Teardown:** `rclc_lifecycle_node_fini`.

   Three things differ from rclcpp, and D4's C surface follows upstream
   rather than rclcpp:
   - **Callback shape.** A callback is `int (*)(void)`: no context and no
     previous state. `RCL_RET_OK` means success, and any other value takes
     the transition's FAILURE branch. rclc has no ERROR branch and no
     `on_shutdown` / `on_error` register function.
   - **Services answered.** rclc builds on rcl's state machine, which with
     the communication interface enabled creates all five services and
     `transition_event`. rclc answers only three of them: `get_state`,
     `get_available_states` and `change_state`. nano-ros answers all five,
     which a stock rclc client cannot tell apart.
   - **Types.** The API needs `rcl_lifecycle_state_machine_t` and
     `rcl_allocator_t`. The nano-ros C surface provides them as the opaque
     storage types its rclc subset already uses.
3. **`transition_event` per transition (D5).** rcl publishes one event per
   `_trigger_transition` call. It sets only `start_state` and `goal_state`,
   and leaves `transition` and `timestamp` at their initialised values
   (`rcl_lifecycle/src/com_interface.c`,
   `rcl_lifecycle_com_interface_publish_notification`). rclcpp's
   `change_state` triggers twice for a transition, primary → transition state
   and then transition state → result, so it publishes **two events**. A
   callback that returns ERROR adds a third, `errorprocessing` → its result
   (`lifecycle_node_interface_impl.cpp`, `change_state`). rclc's
   `rclc_lifecycle_change_state` also triggers twice. So D5's
   `transition_event` is two events per transition, three on ERROR, with the
   same fields set and the same fields left empty.
