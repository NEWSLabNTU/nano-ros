---
id: 2036
title: "C++ has no rclcpp::MessageInfo: a subscription callback taking (const MsgT&, const rclcpp::MessageInfo&) does not compile"
status: resolved
resolved: 2026-10-25
type: enhancement
area: [api]
severity: low
found: 2026-10-25
related: [phase-488]
---

## What

Upstream rclcpp accepts a subscription callback of the shape
`std::function<void(const MsgT&, const rclcpp::MessageInfo&)>` (and its
`shared_ptr` / `unique_ptr` forms). `MessageInfo::get_rmw_message_info()`
returns the source and received timestamps, the publisher GID and the
sequence numbers.

The nros-cpp surface has no `rclcpp::MessageInfo` type at all. The
metadata reaches C++ only through
`Node::create_subscription_with_info(out, topic, ...)`, whose handler is
`(const M&, const uint8_t* attachment, size_t attachment_len)`: the raw
transport attachment bytes (`Subscription<M>::message_info_trampoline`,
`packages/api/nros-cpp/include/rclcpp/subscription.hpp`). That is a different
verb, and the caller has to decode the attachment. The sub-shape dispatcher
(`SUB_SHAPE_*`) has no MessageInfo arm, so the upstream two-argument callback
passed to `create_subscription` does not compile.

`SubscriptionOptions::message_info` is stored and read by nothing (the ledger
row `cpp:SubscriptionOptions::message_info`).

Rust already has the value type: `nros::MessageInfo` is a flat struct of the
fields the backends populate (`rust:MessageInfo`, adopt-bounded).

## Done when

- A `rclcpp::MessageInfo` exists, mirroring `nros::MessageInfo`'s fields.
- `create_subscription` accepts the upstream two-argument callback shapes and
  delivers them through the with-info path.
- A runtime test on the stub backend shows the fields arrive.
- The `cpp:MessageInfo` and `cpp:SubscriptionOptions::message_info` rows move.

## Resolution

Fixed 2026-10-25.

- **The type.** `rclcpp::MessageInfo`
  (`packages/api/nros-cpp/include/rclcpp/message_info.hpp`) has upstream's
  `get_rmw_message_info()`, in both constnesses. It returns
  `rclcpp::RmwMessageInfo`, which has `rmw_message_info_t`'s members under the
  same names: both timestamps, both sequence numbers, `publisher_gid.data[24]`
  and `from_intra_process`. The type has our own name because the C++ surface
  does not include the RMW ABI headers.
- **The callback shapes.** `create_subscription` detects the four upstream
  two-argument shapes: `(const M&, const MessageInfo&)` and the
  `shared_ptr<const M>`, `shared_ptr<M>` and `unique_ptr<M>` forms. They are
  detected in the same order as the one-argument shapes, and they reuse the
  per-delivery heap copy that issue 1995 added for the pointer forms. A callback
  that matches a one-argument shape keeps the one-argument path.
- **The runtime path.** The executor gained one new verb,
  `Executor::add_arena_subscription_msg_info_closure`. It is the raw entry for a
  caller that deserializes the message itself, and it takes each sample with
  `take_serialized_with_info`, the same take the typed with-info path uses.
  `nros_cpp_subscription_register_message_info` hands the C++ side the CDR bytes
  plus an `nros_cpp_message_info_t` built from the Rust `MessageInfo`. The
  registration owns the heap callable and frees it once, on release.
- **Freestanding builds** refuse the shapes at compile time with
  `NROS_RCLCPP_REFUSE_MESSAGE_INFO_CALLBACK`, because the callable lives on the
  heap.
- **`SubscriptionOptions::message_info`** is now read. Set to `true` with a
  callback that takes no `MessageInfo`, it aborts the create instead of being
  ignored.

**Tests.** Both run in `just check cpp`.

- `subscription_message_info_runtime.cpp` runs on the stub backend, which gained
  `nros_stub_rmw_stage_sample_info` and a `take_with_info` slot. All four shapes
  receive the staged metadata, the `(const M&, …)` case goes through the options
  overload with the flag set, and a sample staged without metadata arrives with
  every field zero. It is also clean under ASan, including the leak check.
- `subscription_message_info_freestanding_probe.cpp` must fail to compile, and
  its error must name the refusal.

**Negative control (measured).** Replacing `from_info(info)` with
`from_info(None)` in `nros-cpp`'s `subscription.rs` gives 4 failures.

## Still open

- A backend that reports no metadata cannot be told apart from one that
  reported zeros: `nros_cpp_message_info_t::has_info` reaches the trampoline,
  but `rclcpp::MessageInfo` has no field for it, since upstream has none.
  Resolved 2026-10-26 by issue 2074: `MessageInfo::is_reported()`.
- The polling path, `PollSubscription<M>::take(M&, MessageInfo&)` (upstream's
  `Subscription::take` with info), is not offered. See `cpp:Subscription::take`.
  Resolved 2026-10-26 by issue 2074.
- Stock ROS 2 was not tested live: the test proves the C++ plumbing on the stub.
  The backends' `take_with_info` population is covered by their own tests
  (issue 1495).
