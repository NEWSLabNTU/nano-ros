---
id: 2349
title: "The component API exposes no MessageInfo, so a component cannot read what a hand-written node can"
status: open
type: limitation
area: api, node
related: [issue-0441]
found: 2026-11-07
---

**Home: none yet.** Found while planning phase-531 W6, the packages/testing
batch ([phase-531](../roadmap/phase-531-workspace-first-project-layout.md)).

## Symptom

A subscription callback in a component (the `Node` / `ExecutableNode` API,
RFC-0043/0044) gets a `CallbackCtx` (`packages/api/nros/src/node.rs`). That
context has no accessor for the message's `MessageInfo`: sequence number,
publisher GID, source timestamp. The receive path already carries it. The
executor's `.message_info()` subscription builder hands a callback
`FnMut(&M, Option<&MessageInfo>)`, and the raw path gets a `RawMessageInfo`
(`nros-node/src/executor/arena.rs`). But a callback reaches that builder
only from a hand-written `main` that opens its own executor.

So a component cannot do what a hand-written node can. Workspace-first
(RFC-0121) makes the component the default shape, and RFC-0122 D10 keeps a
hand-written `main` only where it demonstrates one, so this gap now decides
the shape of a program.

## Evidence

`packages/testing/nros-tests/bins/message-info-observer/src/main.rs`
explains in its module docs why it is a bin with its own `main` and not the
listener example: "`CallbackCtx` exposes no accessor for it; the
`FnMut(&M, Option<&MessageInfo>)` shape lives on the executor's
`.message_info()` subscription builder, which the `Node`/`ExecutableNode` API
a demo is written against never reaches."

Phase-531 W6 is moving that bin to the workspace layout. It has to stay an
owned entry (a hand-written `main` reaching its board through the owned-entry
adapter) for this reason alone.

## What would close it

A `CallbackCtx` accessor for the current message's `MessageInfo` (an
`Option`, since not every backend fills every field), carried by the
generated dispatch the same way the message is. Two things to decide first:

- the cost on the zero-copy trampoline (issue 0441's subject);
- whether the C (`rclc`) and C++ (`rclcpp`) component surfaces get the same
  accessor. Upstream `rclcpp` passes `const rclcpp::MessageInfo&` to a
  subscription callback that asks for one.

Not attempted here.
