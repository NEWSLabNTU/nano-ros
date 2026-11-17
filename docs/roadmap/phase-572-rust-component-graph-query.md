# Phase 572 — a Rust component can query the ROS graph

**Status (2026-11-17). OPEN: W1–W4 are open.** Tracks
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

- **Status:** OPEN

Decide, and record here before building:
- the method names, matching the C and C++ ones;
- which context carries them: `NodeContext` at registration, `CallbackCtx`
  inside a callback, or both;
- what may run inside a callback. A query visits the session's graph cache;
  it must not block or reach back into the executor.

The queries go through the same session path the executor's own queries use,
never a second implementation.

### W2 — The queries on the Rust component surface

- **Status:** OPEN

Implement W1's shape. The dispatch site already holds the executor, so each
method forwards to it.

Acceptance:
- a Rust component calls each query and gets the executor's answer;
- a component that never calls a query costs nothing it did not cost before
  (the image's size, measured before and after).

### W3 — One graph, three languages

- **Status:** OPEN

A test per language (Rust, C, C++), each a component on native Linux over
zenoh, with the same peer on the graph.

Acceptance: all three report the same topics, services and nodes for that
peer.

### W4 — The Rust graph-query stack measurement (issue 2360)

- **Status:** OPEN

Once W2 lands, take issue 2360's missing measurement: the main-thread peak, on
the nucleo-f767zi Renode twin, of a Rust Zephyr image whose component queries
the graph.

Acceptance: the peak is recorded in issue 2360, and fits the 32 KiB main a
Rust entry gets, or a frame fix lands with it.
