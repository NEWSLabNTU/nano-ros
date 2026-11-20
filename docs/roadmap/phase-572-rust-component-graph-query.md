# Phase 572 — a Rust component can query the ROS graph

**Status (2026-11-21). OPEN: W1, W2 and W3 are done (`3e37927b51`); W4 is
open. W2's soundness conditions still hold while
[phase-573](phase-573-executor-reentry-soundness.md) W3/W4 are open.** Tracks
[issue 2674](../issues/2674-rust-component-api-has-no-graph-query.md). Implements
no new RFC. It closes a language-parity gap in the surface RFC-0089 adopted.
Owner: nros 243 (W2 onward, released by nros 133 on 2026-11-20).

## Why

The graph queries (`get_topic_names_and_types`, `get_service_names_and_types`,
`get_node_names`, the four `*_names_and_types_by_node` forms) are reachable from
a C component, through the `rclc_executor_t *` its configure receives, and from
C++ through `rclcpp::Node`. A Rust component gets only `NodeContext` in
`register()` and `CallbackCtx` in `on_callback`, and neither has them. Only the
imperative Rust `Executor` / `Node` do, and no generated entry gives a component
either of those. So a Rust image written the way every template writes one
cannot ask what is on the graph.

## Work items

### W1 — The API shape

- **Status:** DONE (2026-11-17, this PR: the shape below)

Decide, and record here before building:
- the method names, matching the C and C++ ones;
- which context carries them: `NodeContext` at registration, `CallbackCtx`
  inside a callback, or both;
- what may run inside a callback. A query visits the session's graph cache;
  it must not block or reach back into the executor.

The queries go through the same session path the executor's own queries use,
never a second implementation.

Decided 2026-11-17:
- **Names and signatures:** the 14 the Rust executor's `Node` has, which are
  rclrs's: the four counts (`count_publishers`, `count_subscriptions`,
  `count_services`, `count_clients`), `get_node_names`,
  `get_node_names_with_enclaves`, `get_topic_names_and_types`,
  `get_service_names_and_types`, the four `get_*_names_and_types_by_node`
  forms, and `get_{publishers,subscriptions}_info_by_topic`. They take the
  same visitor signatures and return the same `NodeError`.
- **`TickCtx`:** yes. The tick driver already reaches the executor through
  its raw pointer and reborrows it per call, as the action and client
  dispatch do.
- **`NodeContext`:** yes. At boot the graph is mostly not discovered yet, so
  the docs say an early answer is partial, never an error.
- **`CallbackCtx`:** no. A callback runs inside the pass, and giving it the
  session would alias the executor (issue 2675). A component that needs the
  graph queries it from its tick. Phase-573's restructure may later make a
  sound callback path possible; that is a separate design.

### W2 — The queries on the Rust component surface

- **Status:** DONE (2026-11-21, `3e37927b51`)

Implement W1's shape. The dispatch site already holds the executor, so each
method forwards to it.

**Not blocked on phase-573 (ruling of 2026-11-20, nros 133).** The
`TickCtx` / `NodeContext` path is sound today. The owned runtime calls
`run_ticks()` after `spin_once` returns, so no executor `&mut` is live across a
tick, and each tick reborrows the executor per call the way the action and
client dispatch do. On the install path, `component_tick_trampoline` runs
inside the pass, which 573 W2 made re-derive after every callback and every
tick. None of 573's open items (W2b's `*_try_process`, W5d, W5e) touch a tick.
The block applied to a `CallbackCtx` variant, which W1 ruled out. Two
conditions keep it so while 573 W3/W4 are open:
- **No session pointer outlives a query.** Never `Executor::session_ptr()` held
  past the call (the temporary-reborrow class 573 W3 fixes): each query
  reborrows the executor and calls its graph method inside that one reborrow,
  and nothing caches a session pointer in the context or the dispatch struct.
- **W4 is inherited, not added.** A band-level tick reborrowing the executor
  while another level runs is the exposure the existing action and client
  tick ops already have; the graph queries take 573 W4's status with them.

Shape as built: a `GraphQuery` trait (the fourteen, executor visitor
signatures) in the doc-hidden `nros::__macro`, answered by the executor's own
graph methods; `TickCtx` holds the tick driver's `RuntimeGraph`, which carries
only the executor's raw pointer and reborrows it inside each call;
`NodeContext` forwards to the executor it holds. One macro writes the fourteen
methods on both contexts.

Acceptance:
- a Rust component calls each query and gets the executor's answer;
- a component that never calls a query costs nothing it did not cost before
  (the image's size, measured before and after).
- a Miri case in `nros-node`'s `executor::tests::reentry` (run by
  `just check miri-executor-reentry`): an enrolled tick queries the graph
  through the raw executor pointer during a pass. Its negative control, a
  session pointer taken before the pass and reborrowed inside it, is reported
  by Miri as undefined behaviour.

### W3 — One graph, three languages

- **Status:** DONE (2026-11-21, `3e37927b51`, built with W2)

A test per language (Rust, C, C++), each a component on native Linux over
zenoh, with the same peer on the graph.

Acceptance: all three report the same topics, services and nodes for that
peer. `tests/graph_query_parity.rs` runs the three as components of one image
beside a zenoh talker, and asserts that they give one answer.

That probe found issue 2775: on zenoh, all three agreed on
`std_msgs::msg::dds_::String_` for `/chatter`'s type, where ROS 2 reports
`std_msgs/msg/String`. They agreed because they share one backend. The fix
demangles where a type leaves the zenoh shim, and the probe now asserts the
ROS form.

### W4 — The Rust graph-query stack measurement (issue 2360)

- **Status:** OPEN (W2 landed)

Once W2 lands, take issue 2360's missing measurement: the main-thread peak, on
the nucleo-f767zi Renode twin, of a Rust Zephyr image whose component queries
the graph.

Acceptance: the peak is recorded in issue 2360, and fits the 32 KiB main a
Rust entry gets, or a frame fix lands with it.
