# Phase 554 — Per-node lifecycle

**Status (2026-11-12). OPEN: every RFC-0128 question is ruled; no
implementation item has started.** This phase is the work for
[RFC-0128](../design/0128-per-node-lifecycle.md): REP-2002 lifecycle as stock
ROS 2 does it, one machine per managed node under the node's own name. Every
question was ruled on 2026-11-12 ("Take your advice on lifecycle questions",
"Hide the node as default"). Phase B (W1-W9: machines and gating) comes first
and unblocks the lifecycle lab of
[issue 2269](../issues/2269-no-emulator-chapters-for-lab-topics.md). Phase C
(W10-W14) adds the callbacks.

RFC-0128 becomes Stable only after W0's three upstream verifications (W15).
Each item is claimed with `just claim phase-554-wN` before it starts.

Every message an item adds that a user can see names the node, the knob or the
transition, and carries no issue, phase or RFC id (user ruling 2026-11-12).
Ids go in source comments.

**Which CI tier a lifecycle PR runs** (user ruling 2026-11-13, "Go option c
for CI tier for the lifecycle PRs"):

- A PR runs tier 2 itself (`just build-test-fixtures lane=tier2`, then
  `just ci matrix`) when it touches the C or C++ API or FFI, generated code,
  a board crate, or embedded sizing and pools.
- A PR that changes only Rust runtime logic, covered by unit tests and the
  Linux e2e cells, waits for one batch tier-2 run at the end of phase B
  (W1-W9) and another at the end of phase C (W10-W14).
- Each PR body states which rule applied and why.
- Catch-up: W1 (a new error mapped into C and C++) and W2 (new FFI, removed
  C++ symbols) both touched C/C++. One tier-2 pass on main covers both.

---

## Why

The measurement in
[issue 2428](../issues/2428-workspace-lifecycle-is-one-machine-per-image.md)
is a workspace image whose one lifecycle machine was named
`/nros_app`, while `ros2 lifecycle get /sensor_node` said `Node not found` and
the node published from boot in `unconfigured`. The user's direction is
"interface compatibility among nano-ros and stock ROS 2".

## Work items

### W0 — Read the three stock behaviours from upstream source

Status: DONE (2026-11-12). The answers are recorded in RFC-0128's
"Verification before Stable", against the upstream branch tips named there:

1. rclcpp activates managed entities only through the base
   `LifecycleNode::on_activate`, on both distros.
2. rclc's API is byte-identical on Humble and Jazzy. Its callbacks are
   `int (*)(void)`, and it answers three of the five services.
3. Two `transition_event` messages per transition, three on ERROR.

Gate: none. W3, W6 and W13 read its answers, so it came first.

Read rclcpp and rclc sources on Humble and Jazzy, not memory, for each of the
RFC's verification items:

1. Whether rclcpp activates managed entities by itself, and from which distro.
   nano-ros activates on every distro (Q2); this records where nano-ros does
   more than rclcpp.
2. Upstream `rclc_lifecycle`'s spellings and callback signatures.
3. How many `transition_event` messages one transition publishes.

Acceptance: each answer recorded in RFC-0128's "Verification before Stable",
with the upstream file and tag it was read from.

### Phase B — machines and gating

### W1 — One machine per managed node (D1)

Status: DONE (2026-11-13).

- **The table.** The executor's lifecycle slot is now a heap table of
  `LifecycleEntry { node, state }`. The `Executor` value holds one `Vec`
  header, which keeps it within issue 0961's size ceiling.
- **Its bound.** The table holds at most `MAX_LIFECYCLE_MACHINES`, which is
  the node table's bound (`NROS_EXECUTOR_MAX_NODES`). That is the same bound
  the lifecycle inbox rings already use, so no new knob was needed.
- **Refusals.** A second registration for the same node is
  `NodeError::LifecycleAlreadyRegistered`, mapped to `RCLC_RET_ALREADY_EXISTS`
  in C and `NROS_CPP_RET_ALREADY_EXISTS` in C++. A full table is
  `NodeTableFull`, whose message names the knob.
- **Accessors.** Per-node `lifecycle_state_machine_on[_mut]` and
  `register_lifecycle_node_on`. The executor-scoped accessors answer for the
  machine on the executor's identity, or for the only machine there is,
  until W3 and W12 move their callers.

- The executor's single `lifecycle` slot becomes a table keyed by `NodeId`.
- Its capacity, `MAX_LIFECYCLE_NODES`, is derived, and is 0 for an image with
  no managed node.
- Registering a node that already has a machine is refused. A full table is
  refused naming the knob.
- The accessors take the node.

Acceptance: unit tests show two machines with independent states. A second
registration of the same node is refused, not replaced, and a table one short
is refused naming the knob.

### W2 — Which nodes are managed (D2, Q1)

Status: DONE (2026-11-13).

- **Which nodes are managed.** `nros_orchestration_ir::overlay::managed_nodes`
  returns them in launch declaration order: every node the resolved model
  marks `lifecycle`, filtered by placement. Each one's autostart is its own
  value, else the image's `[lifecycle] autostart`.
- **Registration.** Every road registers each managed node right after
  building that node:
  - Rust: `apply_lifecycle_node`;
  - C and C++: `nros_cpp_lifecycle_register_node(executor, ns, name, code)`.

  The call is per node, not one call over the list, because a multi-level
  entry builds each node on its own level. Nothing registers on the
  executor's own identity. `apply_lifecycle(u8)` and
  `nros_cpp_lifecycle_autostart` are gone.
- **Self-managed nodes.** A node whose component registered its own machine
  (a C++ `rclcpp_lifecycle::LifecycleNode`) keeps it.
- **A leaf with no model.** ~~Its `lifecycle` manages the first node the
  image built.~~ Withdrawn by the 2026-11-13 ruling; see W2.b.
- **Examples.** The three feature-workspace lifecycle examples now declare
  `<lifecycle_node>`.
- **Acceptance.** Measured on linux/zenoh: the C, C++ and Rust lifecycle
  cells (`workspace_features_e2e`) now assert that `ros2 lifecycle nodes`
  lists exactly `/talker`, and all three pass. A negative control (expecting
  another name) fails with the real list `["/talker"]`.
- **Not yet measured:** the Zephyr native_sim lifecycle cell.

The resolved model's per-node `lifecycle` fact is the one list the entry reads,
on all three roads (`nros::main!`, C codegen, C++ codegen). Its inputs:

- launch `<lifecycle_node>` / `LifecycleNode(...)`;
- a contract's `lifecycle: true`;
- a C++ component deriving from `rclcpp_lifecycle::LifecycleNode`.

A plain component launched as `<lifecycle_node>` is managed, with default
callbacks (Q1).

Acceptance: a workspace with one managed and one unmanaged node registers
exactly one machine, on each road, and the managed node's model row says so.

### W2.b — The code decides which nodes are managed (D2, ruled 2026-11-13)

- **Status:** OPEN. Part 1 (the run-time half) is done; part 2 (the probe
  field) is open.

Two rulings, both 2026-11-13. The first: "Go option c for lifecycle node that
code decides, as in ROS 2". The second, on how the build learns it: "reuse
information the component registration already reports". RFC-0128 D2
records both.

A node is managed when its own code uses the lifecycle API (Rust
`LifecycleComponent`, C `rclc_lifecycle`, C++ `rclcpp_lifecycle::LifecycleNode`)
or when the launch model marks it `<lifecycle_node>`. Every other node stays
plain and publishes at once, with or without a launch file. Nothing is
inferred from build order.

**Part 1: run time (done).**

- `Executor::drive_code_managed_autostart(autostart)` drives every node that
  holds a machine its own code registered, in node-table order. A node with
  no machine is left alone.
- A single-package leaf's `lifecycle` axis (`apply_lifecycle_self_bringup`)
  calls it. W2's "manage the first node built" is removed.
- Unit tests in `nros-node` (`with_no_launch_file_*`) build the plain node
  first, so a build-order rule fails them. A mutation that restores
  "first node" fails both tests.

**Part 2: the probe field (open).**

- The registration observer reports a lifecycle registration, with its node.
- The metadata probe writes it as one per-node field in its sidecar, beside
  `in_place`. Both directions are gated as the other probe facts are: a
  registration the sidecar does not record fails, and so does a recorded
  field nothing registered.
- The model road folds it into each node's `lifecycle` fact
  (`overlay::managed_nodes`).
- The leaf road (`--from-leaf`, the single-package cargo leaf) reads it from
  the same sidecar for pool and queryable sizing.
- Sources are not scanned, and no component declares the fact by hand.

Acceptance (all three roads, on linux/zenoh):

- A standalone leaf whose node uses the lifecycle API appears in
  `ros2 lifecycle nodes`.
- A standalone plain talker does NOT appear there, and publishes immediately.
- A two-node leaf manages exactly the node that uses the API.
- A launch file that marks a node with plain code `<lifecycle_node>` gets a
  machine with default callbacks (Q1).
- A node whose code uses the API, with no launch mark, takes the image's
  `[lifecycle] autostart` (D6, on W7's first spin pass).

The Rust and C cells need the language API first: Rust `LifecycleComponent`
is W10 and C `rclc_lifecycle` per node is W11. C++ `LifecycleNode` already
registers per node, so its cell can land with part 2.

### W3 — The image announces only its declared nodes (D8)

Status: OPEN.

Each row of RFC-0128's D8 table, as ruled:

- **Dropped:** zenoh's primary node token at session open.
- **Moved to the primary node:**
  - the cffi and executor-level `node_id == None` fallbacks;
  - the contract `/diagnostics` reporter;
  - remap resolution.
- **Dropped:** the node-table spare slot.
- **rclc:** a node gets a node record at init, and an empty name is refused
  (issue 2429).
- **Zero-node direct-API images:** keep their configured name as their one
  node, and are refused at open with no name.

Acceptance:

- `ros2 node list` against a workspace image shows exactly its declared nodes
  on zenoh, Cyclone and XRCE, from boot.
- No `/nros_app` and no `//get_state` anywhere.
- `interop_e2e`'s lifecycle example declares its node.

### W4 — zenoh: one shared lifecycle queryable and a dispatcher (D7)

Status: OPEN.

- One queryable per image, on a wildcard key covering the five service keys of
  all its managed nodes.
- A dispatcher routes each request to a machine by the node and the service in
  the request key.
- Its inbox is sized for lifecycle requests.
- `get_available_states`, `get_available_transitions` and
  `get_transition_graph` are built on demand from const tables.
- Each managed node keeps its own liveliness tokens: five services, plus the
  `transition_event` topic.
- `transition_event` is an on-demand put on `/<node>/transition_event`, not a
  held publisher.

Acceptance:

- With three managed nodes, `ros2 service list -t` and `ros2 topic list -t`
  show per node exactly what an rclcpp `LifecycleNode` shows.
- `ros2 lifecycle nodes/get/set/list` behave as in W9.
- The image declares one lifecycle queryable, read from the shim's count.

### W5 — DDS backends and the derived pools (D7)

Status: OPEN.

- **Cyclone and XRCE:** each managed node gets its own five service reader and
  writer pairs and its `transition_event` writer (cost accepted, ruled
  2026-11-12).
- **Derived pools.** Every count of the lifecycle family becomes per managed
  node: `InfraServices`, `nros_sizing_descriptor::infra`, `nros-zpico-build`'s
  runner, `executor_sizing`, and the inbox rings. On zenoh that is 1 queryable
  per image and 6 tokens per node; on DDS it is the entities per node.
- The `ZENOH_MAX_QUERYABLES` help text in `knobs.toml` is corrected.

Acceptance:

- The sizing descriptor of an image with N managed nodes states those counts.
- An image with no managed node states zero for the family.
- Images derived to the stated pool sizes boot with no pool-full refusal.

### W6 — Managed publishers follow the node's state (D3, Q2)

Status: OPEN. Gate: W0 item 1 for the book's wording only, not for the
behaviour.

- Every publisher a managed node creates is activated when the node enters
  `active` and deactivated when it leaves, on every distro (Q2).
- While inactive, `publish` sends nothing and returns success, logging once.
- Subscriptions, services, timers and clients are not gated.

Acceptance: W9's silent-until-active cell.

### W7 — Per-node `autostart` on the first spin pass (D6)

Status: OPEN.

- Each managed node takes its own `lifecycle_autostart`, defaulting to the
  image's `[lifecycle] autostart`.
- Its transitions run on the executor's first spin pass, in launch
  declaration order, never before the spin loop (ruled 2026-11-12).
- `reduce_lifecycle_autostart`'s one-value rule (phase-486 W0) is retired.

Acceptance:

- Two managed nodes with `autostart` `active` and `none` boot to `active` and
  `unconfigured` respectively.
- A publish in `on_activate` reaches a host subscriber, which shows the
  transport was live.

### W8 — The book's lifecycle chapter

Status: OPEN.

The user-guide chapter states:

- managed nodes;
- `autostart`;
- publisher gating;
- the zenoh and DDS cost per node, from W9's measurement;
- where nano-ros activates publishers and rclcpp does not (W0 item 1).

It unblocks issue 2269's lab text.

Acceptance: the chapter's `ros2 lifecycle` blocks are taken from a W9 run, not
written by hand.

### W9 — Phase B acceptance cells and the RAM measurement (D9)

Status: OPEN.

**The cells** are `interop::CELLS` rows, zenoh first and then every RMW the
interop matrix carries. Each runs the same `ros2 lifecycle` commands against
stock rclcpp's demo `lifecycle_talker` on the host and against the image, and
compares:

- `nodes` (exactly the managed nodes);
- `get` after each transition;
- `list` in each primary state;
- `set`, for legal and illegal transitions;
- `ros2 node list` (exactly the declared nodes);
- two managed nodes with independent states;
- **silent until active:** with `autostart = "none"`, nothing is published
  until `activate`; publishing stops at `deactivate` and resumes at the next
  `activate`.

`workspace-zephyr-rs-lifecycle` gains the silence assertion.

**The RAM measurement:** `just mem-report` on the smallest supported board,
comparing an image with N managed nodes against one with N+1, for zenoh,
Cyclone and XRCE. The numbers are written into RFC-0128 D7 and the W8 chapter.

Acceptance:

- Every cell is green in its lane.
- The per-node RAM delta is recorded for each backend, on the named board,
  with the commit it was measured on.

### Phase C — callbacks

### W10 — Rust `LifecycleComponent` (D4)

Status: OPEN.

- A trait beside `Component`, with rclcpp's defaults: Success, and Failure for
  `on_error`.
- Each method takes the previous state and a `TransitionCtx` giving the node,
  its publishers and its parameters.
- The state is readable from the context.
- A component implementing it is managed (the D2 input).

Acceptance: a Rust workspace component's `on_configure` creates its publisher
and `on_activate` logs. Returning FAILURE keeps the node in its state, and
both are observable from the host.

### W11 — C: upstream `rclc_lifecycle` (D4)

Status: OPEN. Gate: W0 item 2.

- The per-node API with upstream's spellings and signatures.
- A C component calls it from its configure function on its own node.
- `rclc_executor_register_lifecycle_services` and its two siblings retire.

Acceptance: the same behaviour as W10 for a C component, and an rclc program
written against upstream's header compiles unchanged.

### W12 — C++: `LifecycleNode` bound per node (D4)

Status: OPEN.

- `rclcpp_lifecycle::LifecycleNode::start()` registers its own node's machine,
  which adds a table entry.
- Every publisher it creates is a `LifecyclePublisher`.

Acceptance: two `LifecycleNode`s on one executor have independent states, and
the W10 behaviour holds in C++.

### W13 — State-machine conformance (D5)

Status: OPEN. Gate: W0 item 3.

- **Automatic error processing:** a callback returning ERROR runs `on_error`
  at once. Its SUCCESS goes to `unconfigured` and its FAILURE to `finalized`.
- **Transition states** (10-15) are visible inside callbacks.
- **`transition_event`** carries what rclcpp publishes for the same
  transition.

Acceptance: unit tests per rule. A cell drives a callback that returns ERROR
and compares the observed states and `transition_event` messages against
rclcpp.

### W14 — Phase C cells per language

Status: OPEN.

W9's cells in Rust, C and C++, plus callbacks returning SUCCESS, FAILURE and
ERROR.

Acceptance: every cell is green in its lane.

### W15 — RFC-0128 Stable

Status: OPEN. Gate: W0, and W1-W14 done.

- RFC-0128 flips to Stable, with W0's answers recorded.
- ARCHITECTURE.md is updated.
- RFC-0036's divergence line is narrowed.
- Issues 2428 and 2429 are archived as resolved.

Acceptance: the RFC status and ARCHITECTURE change land in one commit, and
both issues are archived.

## Known issues found on the way

- [Issue 2518](../issues/2518-census-entities-attach-to-last-opened-node.md):
  the metadata census records each entity on the node opened last, not on
  the node that owns it. Seen while writing W2.b's census test. W5's
  per-node lifecycle sizing reads the census per node, so it needs this
  fixed or ruled out first.
