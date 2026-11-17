# Phase 572 — a Rust component can query the ROS graph

**Status (2026-11-17). OPEN: W1 is done; W2–W4 wait on
[phase-573](phase-573-executor-reentry-soundness.md).** Tracks
[issue 2674](../issues/2674-rust-component-api-has-no-graph-query.md). Implements
no new RFC. It closes a language-parity gap in the surface RFC-0089 adopted.
Owner: nros 133.

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

- **Status:** BLOCKED (on phase-573, issue 2675)

Implement W1's shape. The dispatch site already holds the executor, so each
method forwards to it.

Acceptance:
- a Rust component calls each query and gets the executor's answer;
- a component that never calls a query costs nothing it did not cost before
  (the image's size, measured before and after).

### W3 — One graph, three languages

- **Status:** BLOCKED (on W2)

A test per language (Rust, C, C++), each a component on native Linux over
zenoh, with the same peer on the graph.

Acceptance: all three report the same topics, services and nodes for that
peer.

### W4 — The Rust graph-query stack measurement (issue 2360)

- **Status:** BLOCKED (on W2)

Once W2 lands, take issue 2360's missing measurement: the main-thread peak, on
the nucleo-f767zi Renode twin, of a Rust Zephyr image whose component queries
the graph.

Acceptance: the peak is recorded in issue 2360, and fits the 32 KiB main a
Rust entry gets, or a frame fix lands with it.
