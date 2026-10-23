---
id: 1995
title: "A ported rclcpp subscription callback taking `Msg::UniquePtr` / `Msg::ConstSharedPtr` does not compile: generated C++ messages carry no pointer aliases and the arena delivers `const Msg&`"
status: resolved
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

## Resolution (2026-10-23)

Took the first of the two options above — a heap copy per delivery, hosted
only, opted into by the callback's signature — and the second for
freestanding builds.

- **Codegen.** `packs/cpp/_pointer_aliases.jinja` emits rosidl's
  `SharedPtr` / `ConstSharedPtr` / `UniquePtr` / `ConstUniquePtr` / `WeakPtr` /
  `ConstWeakPtr` inside `#if defined(NROS_CPP_HAS_SHARED_PTR)` on every
  message, service Request/Response and action Goal/Result/Feedback; the three
  headers include `rclcpp/detail/std_detect.hpp`. No codegen-version bump: the
  change is additive, and both mixed pairs compile (an old tree just lacks the
  aliases; a new tree needs only `std_detect.hpp`, which every accepted runtime
  has). No committed generated C++ exists; the golden corpus is regenerated.
- **Dispatch.** `create_subscription<M>(topic, qos|depth, cb)` classifies the
  callable (`detail::subscription_callback_shape`, `rclcpp/subscription.hpp`):
  `const M&` (also by value / generic) passes through untouched; on a hosted
  build `std::shared_ptr<const M>`, `std::shared_ptr<M>` and
  `std::unique_ptr<M>` callables are wrapped in an adapter that copies each
  delivered message into `std::make_shared<M>` / `std::unique_ptr<M>`. The
  arena still stores `InplaceFn<void(const M&)>`; the delivery path is
  unchanged.
- **Not the service handler's loan.** `detail::lend_from_frame` is NOT reused:
  as stated above, keeping the pointer is the common subscription case, and a
  loan aborts on it. With the copy, retention is legal; the runtime test keeps
  a pointer past the frame's death and reads the delivered contents.
- **Freestanding / anything else.** A callable taking none of the shapes is
  refused at the call with `NROS_RCLCPP_REFUSE_SUBSCRIPTION_CALLBACK_SHAPE`,
  which names `const M&` — before this it was a conversion error inside
  `InplaceFn`. Without `NROS_CPP_STD` that includes the pointer shapes.

Tests (`just check cpp`): `subscription_pointer_callback_runtime.cpp` (each
shape classifies and delivers, `std::bind` over a ConstSharedPtr member, a kept
pointer outlives the frame, each delivery a fresh copy);
`subscription_pointer_callback_freestanding_probe.cpp` and
`ros2_refuse_subscription_callback_probe.cpp` (expected failures, grepped for
the refusal). Acceptance: `rclcpp-examples/edits.json` —
`minimal_subscriber_lambda` 4 -> 0 (the port is the upstream file verbatim),
`multithreaded_executor` 26 -> 18; 226 -> 214 edited lines in total. Ledger:
`cpp:Node::create_subscription` records the shape and the remaining divergence
(one allocation + copy per pointer-shaped delivery).
