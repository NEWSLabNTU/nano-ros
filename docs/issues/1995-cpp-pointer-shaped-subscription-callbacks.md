---
id: 1995
title: "A ported rclcpp subscription callback taking `Msg::UniquePtr` / `Msg::ConstSharedPtr` does not compile: generated C++ messages carry no pointer aliases and the arena delivers `const Msg&`"
status: open
type: limitation
severity: medium
area: [api, codegen]
related: [phase-488]
found: 2026-10-21
---

## What happens

The most common subscription callback in real rclcpp code takes the message
by smart pointer:

```cpp
[this](std_msgs::msg::String::UniquePtr msg) { ... msg->data ... }      // jazzy minimal_subscriber
void subscriber1_cb(const std_msgs::msg::String::ConstSharedPtr msg);    // multithreaded_executor
```

Neither compiles against nros-cpp. Two independent gaps:

1. **Codegen.** `packages/cli/rosidl-codegen/packs/cpp/message.hpp.jinja`
   emits no `SharedPtr` / `ConstSharedPtr` / `UniquePtr` nested aliases, so
   `String::UniquePtr` is "has not been declared" before overload resolution
   is reached.
2. **Dispatch.** `Node::create_subscription` stores an
   `InplaceFn<void(const M&)>`: the arena hands the callback a reference to a
   message decoded into a dispatch-local buffer. Nothing builds a
   `std::shared_ptr<const M>` or `std::unique_ptr<M>`.

Measured by phase-488's `rclcpp-examples` fixture: 4 edited lines in the
jazzy `minimal_subscriber_lambda`, and 4 of the 26 in
`multithreaded_executor` (`edits.json`, cause "pointer-shaped subscription
callback").

## Why it is not a drop-in

A `ConstSharedPtr` callback may legally KEEP the pointer (`last_msg_ = msg;`
is common in real nodes). The service handler phase-488 adopted lends its
objects from the dispatch frame and aborts if the handler keeps one, which is
acceptable there because retaining a request is rare. For subscriptions it is
the common case, so the same trick would turn a frequent pattern into a
runtime abort. The honest options:

- copy into a heap `std::make_shared<const M>` per delivery on hosted
  builds only — an allocation per message, opt-in by the callback's
  signature; or
- a compile-time refusal naming `const M&`, which at least replaces the
  "has not been declared" error with the migration.

Either needs the codegen aliases (behind `NROS_CPP_HAS_SHARED_PTR`, the
both-probes `<memory>` rule).

## Acceptance

`edits.json` for `minimal_subscriber_lambda` drops to 0, or the refusal's
text is what a ported file sees.
