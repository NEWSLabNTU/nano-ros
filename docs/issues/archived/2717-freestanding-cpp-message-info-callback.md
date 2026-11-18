---
id: 2717
title: "A freestanding C++ build refuses a subscription callback taking `const rclcpp::MessageInfo&`, though nothing in it needs the hosted library"
status: resolved
type: limitation
severity: medium
area: [api, cpp]
found: 2026-11-18
resolved: 2026-11-19
resolved_in: "feat(nros-cpp, issue 2717): a freestanding C++ build takes a (const M&, MessageInfo) subscription callback"
related: [2349, 2712, 2036]
---

**Home: none yet.** Priority band 2, per-language parity (manager,
2026-11-18). Split out of issue 2349, which closed with Rust and C able to
read a message's metadata and C++ able to only on a hosted build.

## Symptom

On a hosted build (`NROS_CPP_STD`) a subscription callback may take
upstream's `const rclcpp::MessageInfo&` after the message (issue 2036). A
freestanding build refuses it at compile time:

> a subscription callback taking const rclcpp::MessageInfo& is REFUSED by
> nano-ros on a freestanding build: the metadata path keeps the callable on
> the heap.

(`NROS_RCLCPP_REFUSE_MESSAGE_INFO_CALLBACK`, `rclcpp/logging.hpp`, asserted
in `rclcpp.hpp`.) Embedded is nano-ros's main target, so on the boards that
matter most a C++ program has no way to read a message's metadata.

## What blocks it: who owns the callable, not the STL

- `rclcpp::MessageInfo` and `RmwMessageInfo` (`rclcpp/message_info.hpp`) are
  plain structs. Nothing in them needs the hosted library.
- The `const M&` + `MessageInfo` shape detection
  (`sub_info_shape_ref`, `subscription.hpp`) is already freestanding. Only
  the smart-pointer message forms sit behind `NROS_CPP_HAS_SHARED_PTR`, which
  is right: those need `shared_ptr`/`unique_ptr`.
- The ONE blocker is `register_subscription_message_info`
  (`subscription.hpp`). It heap-allocates the callable (`new Fn(...)`) and
  hands the runtime a `drop` that deletes it
  (`nros_cpp_subscription_register_message_info`). A build with no allocator
  cannot do that.

The plain one-argument callback already solved the same problem without a
heap. `register_subscription_capturing` hands the callable's BYTES to
`nros_cpp_subscription_register_capturing`, which copies them into the
executor arena (`add_arena_subscription_c_callback_with_capture`) and runs
the callable's destructor on release. The metadata path has no arena-capture
twin.

## The fix is small

Three pieces, all on the existing `BufferedRawInfo` executor path (the one
Rust components, C's `rclc_executor_add_subscription_with_message_info` and
hosted C++ already share):

1. An executor entry that copies a capture into the arena and calls a C
   function pointer with `(data, len, info, capture)`. It is the metadata
   twin of `add_arena_subscription_c_callback_with_capture`, on the
   `take_serialized_with_info` take.
2. An FFI entry, `nros_cpp_subscription_register_capturing_message_info`,
   the twin of `nros_cpp_subscription_register_capturing`.
3. A freestanding branch of the `create_subscription` dispatch for the
   `const M&` + `MessageInfo` shape that registers through it with an
   `rclcpp::InplaceFn`, as the one-argument freestanding path does. Then the
   refusal macro is deleted for that shape. The smart-pointer + `MessageInfo`
   shapes stay hosted, refused by the existing smart-pointer refusal.

## Acceptance

- A freestanding build (`just check cpp`'s freestanding syntax lane) accepts
  a `(const M&, const rclcpp::MessageInfo&)` callback.
- A test on a freestanding image (or the C++ stub-backend run test) shows
  staged metadata reaching the callback. It must fail when the callback is
  handed a default `MessageInfo`, as `subscription_message_info.c` does for
  C.

## Resolution (2026-11-19)

Fixed as planned: the arena capture, mirroring the one-argument callback's.

- `register_subscription_buffered_raw_msg_info_on` takes an optional capture.
  It is copied to the entry's trailing region after the receive buffer, on
  the next `u64` boundary, and released with the entry. The public face is
  `Executor::add_arena_subscription_msg_info_closure_with_capture`.
- `nros_cpp_subscription_register_capturing_message_info` is the FFI twin of
  `nros_cpp_subscription_register_capturing`; its sink runs the C++
  destructor on the arena copy once.
- The two-argument `create_subscription_dispatch` registers an `InplaceFn`
  through it on EVERY build. The hosted heap path and
  `NROS_RCLCPP_REFUSE_MESSAGE_INFO_CALLBACK` are gone. Pointer-shaped
  message-info callbacks stay hosted, refused by the existing shape refusal.

Tests, in `just check cpp`:
- `subscription_message_info_freestanding_probe.cpp` must COMPILE; the lane
  used to require it to fail.
- `subscription_message_info_freestanding_runtime.cpp`, freestanding, run with
  and without metadata on the stub backend: staged timestamps, sequence
  numbers and GID reach the callback, and a stateful capture survives the
  arena copy.
- Mutation, a default `MessageInfo` handed to the callback: the hosted run and
  the freestanding run each fail 4 checks.

