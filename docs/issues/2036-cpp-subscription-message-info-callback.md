---
id: 2036
title: "C++ has no rclcpp::MessageInfo: a subscription callback taking (const MsgT&, const rclcpp::MessageInfo&) does not compile"
status: open
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
