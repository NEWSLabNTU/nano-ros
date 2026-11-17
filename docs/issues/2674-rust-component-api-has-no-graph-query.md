---
id: 2674
title: "A Rust component cannot query the ROS graph: the C and C++ component surfaces can, and only the imperative Rust Executor has the queries"
status: open
type: limitation
severity: low
area: [api, rust, graph]
related: [issue-2360, issue-2658, issue-0791, phase-379, phase-381, phase-572]
found: 2026-11-17
---

**Home: [phase-572](../roadmap/phase-572-rust-component-graph-query.md).** The
API-parity work it extends was phase-379 (archived), beside phase-381, which
added the graph queries.

## What

The graph queries (`get_topic_names_and_types`, `get_service_names_and_types`,
the four `*_names_and_types_by_node` forms, `get_node_names`) exist on all three
language surfaces, but only C and C++ reach them from the surface an image is
written in:

| language | surface a template / generated entry gives the user's code | graph query there |
| --- | --- | --- |
| C | a component's configure receives the `rclc_executor_t *` (`templates/roles/c/talker.component.c`) | `rclc_executor_get_topic_names_and_types(executor, …)` and its siblings |
| C++ | `rclcpp::Node` | `Node::get_topic_names_and_types(visit, ctx)` (`rclcpp/node.hpp`) |
| Rust | `NodeContext` in `register()`, `CallbackCtx` in `on_callback` (`nros` crate, `node.rs`) | **none** |

In Rust the queries are on `nros_node::executor::Executor` and its `Node`
(`executor/spin.rs`, `executor/node.rs`). A component never holds either:
`nros::main!`, `zephyr_component_main!` and every `nros new` Rust template build
the executor themselves and hand the component only `NodeContext` and
`CallbackCtx`. In-tree, the one Rust caller is the native `graph-probe` test
bin, which opens its own executor.

So a Rust image written the way the templates write one cannot ask what is on
the graph, while the same image written in C or C++ can.

## Deliberate or an omission

No decision record found says the component surface should leave the queries
out: RFC-0089's adoption ledger and phases 379 and 381 discuss the executor and
node APIs and never the Rust component context (searched for `CallbackCtx`,
`NodeContext` and "component" beside the graph queries). Phase-381 put the
queries on the Rust executor and on the C and C++ surfaces; the component
context did not get them. This reads as an omission.

## Consequence found on the way

Issue 2360 asked for the main-stack peak of a Rust Zephyr image that queries the
graph (`ZenohSession::names_and_types_filtered`, 3,524 B static since 2026-11-16).
No such image can be built without a hand-written imperative main, so that
measurement is not taken. When the component surface gets the query, it becomes
possible; issue 2360 points here for it.

## Fix direction

Add the graph queries to `CallbackCtx` (and possibly `NodeContext`) with the
executor's visitor signatures, forwarded to the executor the dispatch site
already holds. `check rmw-api-parity` covers the RMW contract, not this
language-surface level; the acceptance below is the parity check this gap
needs.

## Acceptance

- A Rust component's callback calls `get_topic_names_and_types` and sees a
  topic a peer publishes (native test).
- The table above has a row for every graph query, and each language column
  names its call or a stated reason.
- Issue 2360's Rust graph-query measurement is taken on the nucleo twin, or
  that issue says why it is not needed.
