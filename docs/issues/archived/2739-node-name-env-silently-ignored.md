---
id: 2739
title: "`NROS_NODE_NAME` and `NROS_NODE_NAMESPACE` are read on every Linux road and silently change nothing for a component or application-named node"
status: resolved
type: bug
area: [runtime, configuration]
severity: high
found: 2026-11-19
related: [2714, phase-581]
---

**Home:** [phase-581](../../roadmap/phase-581-ros-env-vars.md) W12 (band 1: a
stated setting silently ignored).

## What

A user who exports `NROS_NODE_NAME` or `NROS_NODE_NAMESPACE` to rename a node
on a Linux process gets no effect and no message. Measured in phase-581 W0
(`docs/roadmap/data/phase-581-w0-measure.md`, the Linux run with both set to
`w0node` and `/w0ns2`): the in-tree Rust, C and C++ `linux` zenoh talkers
each READ both variables, yet the node kept its own name. The Rust liveliness
token stayed `.../%/%/talker` and the C++ boot printed `Node created: talker`.

The cause, as measured: the name the component (Rust) or the application
(C, C++) passes outranks the environment rung on these images, so the
environment value is discarded after it was read. Nothing warns.

## Why it matters

RFC-0140's rule for a RESPECT row is "on a Linux process the environment
overrides the statement, and a mismatch is warned, never silent". These two
rows are read like RESPECT rows and behave like nothing at all: the worst of
both, since the read makes the variable look supported.

## Fix

Phase-581 W12 (the identity rows): either the environment value renames the
node, with the once-per-process mismatch warning of RFC-0140 D8, or the row
becomes IGNORE with a notice naming the authoring place, never a read whose
result is dropped. Acceptance in W12 is a run per road with the variable set
that observes the new name (liveliness token and boot line), not a read.

## Where the read is dropped (2026-11-23)

`nros::env::env_rung` reads both variables into `EnvRung::node_name` /
`namespace`, and `ExecutorConfig::try_resolve_with` takes them over the baked
rung. That answers ONE name: the executor's configured node (the lazy node the
executor creates when an entity needs one). A Rust component's
`create_node("talker")` and a C/C++ application's named node never consult it,
so on every image whose nodes are named in code or by the launch file, the
value is read and discarded. There is a second reason a plain "env wins" cannot
be the rule: an image with several nodes has several names, and one variable
cannot rename them all without making them collide.

## Recommendation for W12 (manager-endorsed, 2026-11-23)

Not landed ahead of W12: the fix belongs on `nros-env` (W10), under RFC-0140.

- **One node in the image:** `NROS_NODE_NAME` renames that node, whatever
  named it (component, application, launch), and a mismatch with the stated
  name warns once per process (RFC-0140 D8).
- **Two or more nodes:** `NROS_NODE_NAME` is IGNOREd, with a once-per-process
  notice that names where a node's name is authored (`system.toml` or a launch
  remap), never a silent drop.
- **`NROS_NODE_NAMESPACE`:** applies to every node of the image, warning once
  when it differs from a stated namespace. A namespace does not collide, so
  the node count does not matter.

Acceptance stays as above: a Linux run per road (Rust, C, C++), single-node
and multi-node, with the variable set, observing the liveliness token and the
boot line.

## Resolution

Fixed 2026-11-27, by the recommendation above, with the node-count question
settled by user ruling: an unstated count IGNORES the name.

- The rule lives in one place, `nros_node::executor::env_identity`. A pure
  `resolve()` makes the decision. `apply()` adds the once-per-process
  warnings. The values are installed once by the hosted edge
  (`nros::env::try_resolve_hosted`, reached by every Linux road).
- The count is `nros_node::config::STATED_NODE_COUNT`, the sizing descriptor's
  stated `[image] node_count` read through `Fact::stated()`. A refused or absent
  field is `None`, never a guess. `MAX_NODES` stays the capacity.
- It is applied where each road fixes a node's identity:
  - `NodeBuilder::build` (Rust, C++, C's `rclc_executor_node_init`);
  - C's legacy `rclc_node_init_ex` / `rclc_node_init_default`, which skip it;
  - `entity_identity` for an image with no declared node;
  - the executor's own default identity in `ExecutorConfig::try_resolve_with`.
    That one used to take `env > baked` blindly, which is the rename that
    happened while every named node was left alone.
- A launch remap is keyed by the stated identity, so a part the environment
  replaced (the name in a one-node image, the namespace everywhere) is not
  compared, and the node's remaps keep applying.
- Tests: `resolve()` covers each branch (1 / 2+ / unstated / namespace). Route
  tests cover a node created through the executor (Rust) and through
  `rclc_node_init_default` (C), plus a launch remap on a renamed node. They are
  mutation-checked: dropping the rule from `build()`, from the C init, or from
  the remap match each turns them red.
