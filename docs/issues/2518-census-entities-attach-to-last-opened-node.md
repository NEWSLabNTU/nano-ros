---
id: 2518
title: "A metadata census records every entity on the node opened LAST, not on the node that owns it, so an entity created after a second node is opened lands on the wrong node row"
status: open
type: bug
severity: medium
area: [metadata, sizing, lifecycle, cpp]
related: [phase-554, issue-1419, issue-1594]
found: 2026-11-14
---

**Home: [phase-554](../roadmap/phase-554-per-node-lifecycle.md)** (W2.b's probe field, where it was seen).

## What

The metadata recorder (`nros::metadata_mode`) keeps one `current_node`, set by
`begin_node`, and `record` / `record_entity` / `record_parameter` file every
entity against it. The node the entity was actually created ON is never
consulted. So when a component opens a second node and then creates an entity
on the first, the census writes that entity under the second node's row.

Nothing refuses or warns. The sidecar is well formed, its counts per IMAGE are
right, and its counts per NODE are wrong.

## Repro (minimal, C++ ABI, metadata backend)

Observed on PR #3775's `census_lifecycle_tests` (nros-cpp, `check
census-hooks-complete`), with a mutated join that printed the sidecar:

1. `nros_cpp_init_rmw("metadata", ...)`.
2. `nros_cpp_node_create_ex` three times: `plain_node`, `coded_node`,
   `launch_marked_node`, in that order.
3. `nros_cpp_register_lifecycle_services_for_node(coded_node)`, which
   creates `/coded_node/change_state` and the other four services, plus the
   `/coded_node/transition_event` publisher.
4. `nros::metadata_mode::to_json(...)`.

Got: `coded_node`'s row is empty. All five `/coded_node/*` services and
`/coded_node/transition_event` sit in `launch_marked_node`'s row, because
`launch_marked_node` was the last node opened. The same holds for any
entity. A publisher created on `plain_node` after step 2 lands there too.

## Why it matters

Two consumers read the census per node, and both trust the row:

- the sizing descriptor's per-node terms (the parameter family is per node,
  phase-426 W3, and RFC-0128 D7 makes the lifecycle family per managed node
  as well, phase-554 W5);
- the contract join (`nros_cli_core::contract_join`), which attributes probe
  rows to contract endpoints by node.

A wrong row is an over-count on one node and an under-count on another, and an
under-count is a short pool.

## Why it is probably latent today (not swept)

The usual component shape creates a node and then that node's entities before
it opens the next one, so the last-opened node IS the owner. The in-tree
components were not swept for the interleaved shape. Issue 1419's re-open
rule (a node opened again becomes current again) keeps sched-task Rust entries
correct too. The defect needs interleaving: two nodes alive, then entities on
the earlier one. A C++ component that owns two `rclcpp::Node`s and builds both
before wiring either does exactly that.

## Fix direction (not done)

Attribute by the owning node, not by recency. Unverified, to check first:
whether the RMW seam hands the recording backend the owning node at
`create_publisher` / `create_service` / ... (the executor's lifecycle funnel
does know its node id). If it does, thread the node's
name and namespace into `record`, and keep `current_node` only as the fallback
for an adapter that genuinely has no node in scope (and say so per call site).
A test in the shape of the repro above, asserting each entity's row, is the
acceptance.
