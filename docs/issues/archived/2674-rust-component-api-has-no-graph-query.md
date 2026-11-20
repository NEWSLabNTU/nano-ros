---
id: 2674
title: "A Rust component cannot query the ROS graph: the C and C++ component surfaces can, and only the imperative Rust Executor has the queries"
status: resolved
resolved: 2026-11-21
resolved_in: "3e37927b51"
type: limitation
severity: low
area: [api, rust, graph]
related: [issue-2360, issue-2658, issue-0791, phase-379, phase-381, phase-572]
found: 2026-11-17
---

**Home: [phase-572](../../roadmap/archived/phase-572-rust-component-graph-query.md).** The
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

## Resolution (2026-11-21)

Fixed by `3e37927b51` (phase-572 W2 and W3). The Rust row of the table above
now reads:

| language | surface | graph query there |
| --- | --- | --- |
| Rust | `NodeContext` in `register()`, `TickCtx` in `tick()` | `ctx.get_topic_names_and_types(visit)` and the other thirteen, on both contexts |

`CallbackCtx` carries none, by phase-572 W1's ruling: a callback runs inside
the pass, and giving it the session would alias the executor (issue 2675). A
component that wants an answer from a callback marks it due and asks in its
`tick()`.

Acceptance, item by item:
- **A Rust component sees a peer's topic.** `tests/graph_query_parity.rs`
  runs a Rust, a C and a C++ component in one image beside a zenoh talker. All
  three give one answer, `/chatter` included, with its type in its ROS form
  since issue 2775. The query is from `tick()`, not a callback, per the ruling
  above.
- **Every query has a row.** The fourteen queries are one set on both Rust
  contexts (`graph_query_forwarders!`), and each has an api-parity ledger row.
- **The nucleo measurement.** Taken (phase-572 W4, recorded in issue 2360): a
  Rust component that runs the graph queries on every tick peaks at 28,340 B of
  main, the same as the stock talker, inside the 32 KiB a Rust entry gets.
