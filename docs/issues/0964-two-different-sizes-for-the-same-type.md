---
id: 964
title: "The C++ header states an ESTIMATED size for every type, including types that have no bound"
status: open
area: codegen
severity: medium
related: [0896, 0939, 0940, phase-403, phase-380]
---

# One type, two numbers, and the wrong one is the one in the header

## What was measured

`rosidl-codegen`'s C++ pack computes `SERIALIZED_SIZE_MAX` with
`compute_serialized_size_max`, which ESTIMATES: a flat 512 per nested message, a
flat default capacity per string, and it ALWAYS returns a value. It therefore
cannot report "unbounded".

Over 120 types in 12 stock ROS Humble packages:

* **81 of 120 have no derived bound, and the C++ header states a size for every
  one of them.**
* Of the 39 that are bounded, the estimate matched the derived bound **zero
  times**: 38 over, 1 under. `geometry_msgs/Twist` reads 1028 against a derived
  64.

On the island entry the same divergence, per type:

| type | C++ header | derived |
| --- | ---: | ---: |
| `autoware_control_msgs/Control` | 2052 | 114 |
| `nav_msgs/Odometry` | 1804 | unbounded, until a cap |
| `autoware_vehicle_msgs/SteeringReport` | 527 | 24 |
| `autoware_adapi_v1_msgs/OperationModeState` | 572 | 27 |

## Why it matters

phase-403 W6 exports the DERIVED bound. The C++ header keeps emitting the
ESTIMATE. The same type now carries two numbers, and the estimate is the one a
user reads, since it is the constant the header advertises and the one
`{Msg}_RX_MAX_SERIALIZED_SIZE` names.

It also violates phase-380's rule directly: a number nobody chose, substituted
where the honest answer is "no bound exists". That rule is why an unbounded type
is a build error at all, and this path quietly opts out of it.

Real cost, not hypothetical: the island's sizing was planned against the
estimate, so the receive buffer, arena and payload classes were all budgeted for
`Control` at 2052 bytes when it serializes to 114.

## Options

1. **Delete the estimator** and emit the derived bound, poisoning the constant
   for an unbounded type exactly as the C pack already does
   (`unbounded_token` + `unbounded_reason`). Behaviour change: a type that is
   unbounded stops having a size constant, which is the point.
2. **Keep both, renamed.** The estimate becomes something honestly named
   (`..._ESTIMATED_SIZE`) and the derived bound takes the load-bearing name.
   Cheaper, but leaves a number nobody chose in the header.
3. **Emit the derived bound only where one exists** and nothing otherwise,
   which is (1) without the poison token.

(1) matches what the C pack does today and what phase-380 requires. It is a
behaviour change with a blast radius across every C++ consumer, which is why it
is filed rather than done.

## Partly addressed 2026-09-01 (phase-408) — the derived number is now IN the header

Option 2, minus the rename. The C++ pack emits `Msg::TX_MAX_SERIALIZED_SIZE` /
`Msg::RX_MAX_SERIALIZED_SIZE` from the derived bound beside the estimate, and an
unbounded type states neither — it carries the poison templates instead, so
"this type has no bound" is now expressible in a C++ header, which it was not
when this issue was filed.

**One consumer moved: `nros::bind_subscription<M, C, Method>`.** It passed
`M::SERIALIZED_SIZE_MAX` as the subscription's `rx_buffer_hint`; it now passes
`nros::rx_size_bound<M>::value`, the derived RX bound.

**What is still open, stated precisely, because the survey behind this issue
undercounted the risk.** Thirty call sites still stack
`uint8_t buf[...::SERIALIZED_SIZE_MAX]` — 28 in `nros-cpp` headers, 2 in the
example workspaces — and they are NOT all transmit scratch:

* **RECEIVE buffers — the dangerous direction, an under-estimate TRUNCATES.**
  `Subscription<M>::try_recv` / `try_recv_validated` (`subscription.hpp:127,153`),
  `Client<Svc>::call`'s response buffer (`client.hpp:131`),
  `Future<T>`/`Stream<T>` (`future.hpp:46,166`, `stream.hpp:54`), the action
  client's result/feedback buffers (`action_client.hpp:147,238`,
  `polling_action_client.hpp:110,148`), the polling action server's goal buffer
  (`polling_action_server.hpp:82`), and `tick_ctx.hpp:125`.
* **TRANSMIT buffers — an over-estimate only wastes stack.** the request
  buffers in `client.hpp` / `service.hpp` / `tick_ctx.hpp`, the action goal and
  result/feedback serialize buffers, and two example workspaces.

`bind_subscription` was fixed here because it feeds a `rx_buffer_hint`, which is
a HINT — the change cannot break anyone. Retargeting the list above cannot make
that claim: the buffer IS the capacity, so switching an unbounded type from the
estimate to the derived bound turns a working (if arbitrarily-sized) call into a
compile error, and none of these has a `_sized` escape hatch the way
`bind_subscription_sized` does. That is the blast radius this issue was filed
for, and it is unchanged. `nros::rx_size_bound<M>` / `nros::tx_size_bound<M>`
are the spellings whoever takes it should use.
