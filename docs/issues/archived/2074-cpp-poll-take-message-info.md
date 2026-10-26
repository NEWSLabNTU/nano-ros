---
id: 2074
title: "C++ PollSubscription has no take(M&, rclcpp::MessageInfo&): upstream's polling take with metadata does not compile"
status: resolved
resolved: 2026-10-26
type: enhancement
area: [api]
severity: low
found: 2026-10-26
related: [2036, phase-488]
---

## What

Issue 2036 added `rclcpp::MessageInfo` and the subscription callback shapes
that take it. The polling half was left out: upstream's
`rclcpp::Subscription::take(MessageT& message_out, rclcpp::MessageInfo&
message_info_out)` has no counterpart on our caller-owned
`rclcpp::PollSubscription<M>`, so a ported polling loop that reads metadata
does not compile. The only poll-side metadata path was
`take_serialized_with_attachment`, which hands back raw transport attachment
bytes for the caller to decode.

A second gap came from 2036's Still open. A backend that reports no metadata
(XRCE, uORB) leaves every field zero, and so does a backend that reported
zeros. The runtime knows which case it is (`nros_cpp_message_info_t::has_info`),
but `rclcpp::MessageInfo` had no field to carry it.

## Done when

- `PollSubscription<M>::take(M&, MessageInfo&)` exists on hosted and
  freestanding builds and fills the message and its metadata from one take.
- A caller can tell "nothing reported" apart from "zeros reported".
- A stub-backend runtime test covers both, mutation-checked.

## Resolution

Fixed 2026-10-26.

- **FFI.** `nros_cpp_subscription_take_with_info(storage, buf, cap, &len,
  &info)` (`packages/api/nros-cpp/src/subscription.rs`) takes one sample
  through `take_serialized_with_info`, the take the 2036 callback path uses,
  and fills an `nros_cpp_message_info_t` with `from_info`. With nothing
  waiting it returns length 0 and an unreported record. An oversized sample is
  counted and refused like the other take flavours (issue 1425).
- **C++.** `PollSubscription<M>::take(M&, rclcpp::MessageInfo&)` and
  `take_sized<Cap>(M&, MessageInfo&)` sit beside `take(M&)`
  (`rclcpp/polling_subscription.hpp`). `info` is written only when a sample
  was taken. Nothing here touches the heap, so the method is offered on
  freestanding builds too, unlike the callback shapes.
- **Reported vs zeros.** `rclcpp::MessageInfo::is_reported()` is a nano-ros
  extension; upstream has no such member. `message_info_from_ffi` sets it from
  `has_info`, so the callback path carries it too. It is true when the
  backend has a per-sample metadata channel and used it: a backend that fills
  the ABI's `take_with_info` slot (Cyclone DDS, the stub), or the Rust
  adapter's side table (zenoh, for a sample that carried an attachment). It is
  false for a backend with no channel (XRCE, uORB). A default-constructed
  `MessageInfo` is unreported; one built from a caller's `RmwMessageInfo`
  counts as reported.

**Test.** `packages/api/nros-cpp/tests/compile/poll_take_message_info_runtime.cpp`,
in `just check cpp`. It is built twice, hosted (C++17, `NROS_CPP_STD`) and
freestanding (C++14). Each build runs twice: on the stub, and on the stub
registered without its `take_with_info` slot
(`nros_stub_rmw_register_without_message_info`). The checks:

- an empty take answers `TryAgain` and leaves `info` untouched;
- a staged sample returns the message and its staged timestamps, sequence
  numbers and GID, with `is_reported()` true;
- a sample staged with no metadata arrives all zero. `is_reported()` is
  still true on the stub, which reported zeros, and false with no slot;
- `take(M&)` still works beside the new overload.

**Mutations (measured, hosted build).**

- `from_info(None)` in the new FFI fails 3 checks in the `info` run.
- `message_info_from_ffi` always passing `false` fails 2 checks in the `info`
  run.
- `message_info_from_ffi` building the record as before, always reported,
  fails 2 checks in the `no-info` run.

## Still open

- `is_reported()` describes the sample's metadata CHANNEL, not each field.
  Cyclone fills the record through `take_with_info` on every take, so a
  Cyclone field that is zero still reads as reported. Per-field validity would
  need an ABI flag that `rmw_message_info_t` does not have upstream either.
- `take(M&, MessageInfo&)` is a C++ poll-side verb only. The C API's
  `rcl_take` already takes an `rmw_message_info_t*`.
