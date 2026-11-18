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

## Progress (2026-11-18)

Ruled (manager, 2026-11-18): opt-in per subscription, so a plain
subscription keeps its zero cost; C and C++ opt in the way stock ROS does, in
a separate PR.

**Rust: done.** `DeclarativeNode::create_subscription_for_callback_name_with_message_info`
declares the subscription, and `CallbackCtx::message_info()` returns the
message's `MessageInfo` in `on_callback` (`None` for every other callback and
from a backend that reports none).

- A third declared shape, `DeclaredSubscriptionShape::BufferedRawInfo`. The
  runtime lowers it to `Executor::register_subscription_buffered_raw_msg_info_on`,
  the body C++'s `(msg, const rclcpp::MessageInfo&)` callback already used
  (`add_arena_subscription_msg_info_closure` now delegates to it). So all three
  languages read the metadata through one executor path.
- Its `in_place_capable()` is `false` (a copying take), and the metadata probe
  states the same, so the build budgets its receive region.
- The node's QoS overrides are folded first, as on the plain declared path.
- Not the ATTACHMENT path (`register_subscription_buffered_raw_info_on`):
  its `RawMessageInfo` metadata is defaulted. The first build used it, and
  the end-to-end test read `seq=0 gid=000…` for every message and failed.

Tests: `tests/component_message_info.rs` (a `Component` in
`bins/message-info-observer`, image `linux_component`, against a talker over
zenohd: every message carries metadata, the sequence rises, one non-zero
GID); `component_registration.rs` (the declaration is recorded and priced as
buffered); `declared_shape` and `CallbackCtx` unit tests.

**C and C++: open.**

- C++ hosted already takes `(msg, const rclcpp::MessageInfo&)` (issue 2036).
  A freestanding build refuses the shape at compile time.
- C's `rclc_executor_add_subscription_raw_with_info` hands the callback the
  wire ATTACHMENT, not a `MessageInfo`. Its metadata is the defaulted kind
  found above, so it cannot answer this issue as it stands.

