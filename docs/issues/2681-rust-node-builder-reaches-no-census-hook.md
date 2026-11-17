---
id: 2681
title: "A Rust node built with `Executor::node_builder(..).build()` or `Executor::create_node` reaches no census hook, so the census does not count it"
status: open
type: bug
area: [testing, params]
severity: medium
found: 2026-11-17
related: [1419, 2670, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md),
"Issues homed here (2026-11-17)". Found while fixing issue 2670, which is the
same gap for parameters.

## What the code does

The census counts a node when `nros::census_hooks::on_node_create` opens it in
the metadata recorder. The C (`rclc_node_init_ex`, `rclc_executor_node_init`),
C++ (`nros_cpp_node_create*`) and declarative Rust (`node_runtime`'s
`create_node`) routes call it. The imperative Rust routes in `nros-node` do
not: `Executor::node_builder(name).build()`, `Executor::create_node`, and the
`create_node_on*` forms. `nros-node` sits below `nros`, so it cannot call the
hook.

In-tree, `nros-tests/bins/param-two-node-talker` builds its three nodes that
way.

## Why it matters

A census of an image whose nodes are built imperatively records no row for
them. The node table is sized from the contract, so the census's check that
the code creates no node the contract omits does not see them, the same UNDER
direction as issue 2670. An entity the RMW backend records for such a node
also has no node to attribute to.

Issue 2670's fix narrows this: a parameter declared imperatively now opens its
node in the recorder (`metadata_mode::record_parameter_declared_on`), so a node
that declares a parameter is counted. A node that declares none is still
missing.

## Possible shape

The 2670 shape: a node-creation observer in
`nros-node`'s `registration_observer`, fired from the node builder's `build()`
and installed by `nros::metadata_mode::listen_for_registrations`. Reopening a
node is already idempotent (`begin_node`, issue 1419), so the declarative
route, which builds through `node_builder` and then calls `on_node_create`,
would not record twice. Arming is already in place: since issue 2670 the
census boot funnel (`nros-board-linux`'s `census_arm`) calls
`listen_for_registrations` before the executor opens, so an observer fired
from the first imperative `build()` is heard.
