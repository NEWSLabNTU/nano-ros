---
id: 2739
title: "`NROS_NODE_NAME` and `NROS_NODE_NAMESPACE` are read on every Linux road and silently change nothing for a component or application-named node"
status: open
type: bug
area: [runtime, configuration]
severity: high
found: 2026-11-19
related: [2714, phase-581]
---

**Home:** [phase-581](../roadmap/phase-581-ros-env-vars.md) W12 (band 1: a
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
