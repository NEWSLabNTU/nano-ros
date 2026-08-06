---
id: 453
title: "no native action cell proves the goal payload was delivered — the example servers ignore or reinvent `order`"
status: open
type: bug
area: testing
related: [issue-0448, phase-329, rfc-0051]
---

## The gap

`native_example_reqresp_e2e` asserts, for each action cell, that the client
logged `ACTION_RESULT_PREFIX` (`"Result received:"`). That line is printed as
soon as the client DECODES a result — including the zeroed default result it
decodes when the goal never reached the server at all.

So no native action cell distinguishes:

- the goal crossed, the server computed, the result came back; from
- the goal was dropped on the wire, and the client decoded 12 zero bytes.

The service cells do not have this gap: they assert `Result of add_two_ints: 5`,
a value the SERVER computed from the request. An action cell cannot make the
equivalent assertion today.

## Why it cannot simply be fixed by asserting the sequence

The three servers do not share a convention, and one of them does not read the
goal at all:

| Server | Sequence for `order = 10` | Is it a function of `order`? |
| --- | --- | --- |
| `examples/native/rust/action-server` | `[0, 1, 1]` — a fixed 3-element frame | **no** |
| `examples/native/cpp/action-server` | 10 elements (`for i < goal.order`) | yes |
| ROS 2 `action_tutorials_py` (interop tests only) | 11 elements (`order + 1`) | yes |

The Rust example server:

```rust
let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
let _ = sequence.push(0);
let _ = sequence.push(1);
let _ = sequence.push(1);
```

`goal.order` is destructured as `_order` and never used. Its result is therefore
byte-identical whether the goal carried `order = 10`, `order = 0`, or never
arrived — the rust cells are structurally incapable of proving delivery.

`FIBONACCI_ORDER_10_SEQUENCE` in `nros_tests::output` encodes the ROS 2
convention (11 elements) and had **zero users** until issue 0448 wired it into
`xrce_ros2_interop`, which is the only action test whose peer computes the
sequence.

## What this cost

Issue 0448: the Rust action client shipped a second CDR encapsulation header, so
every `SendGoal_Request` was 4 bytes over the ROS 2 layout and Fast-DDS dropped
it outright. Every native action cell stayed green throughout — they were
asserting a prefix the client prints regardless. Only the XRCE↔ROS 2 interop
test caught it, and only because a real `rcl_action` server was on the other end.

## Fix directions

1. **Make the Rust example server compute the sequence from `goal.order`**, like
   the C++ one already does. Then every cell's payload is a function of the goal
   and the assertion becomes possible. This is the smallest change with the
   largest coverage gain, but it alters an example's output, so any test grepping
   its current `[0, 1, 1]` frame has to move with it.
2. **Give `roles()` a per-language expected-sequence field** instead of one
   shared constant, so each cell asserts its own server's convention. Necessary
   regardless of (1) unless the C++ server is also changed to `order + 1`.
3. **Align all three servers on the ROS 2 `order + 1` convention.** The most
   faithful to the reference implementation and lets one constant serve every
   cell, at the cost of touching all the example servers and their expected
   output.

(1) + (2) is the recommended pair: it keeps each example's own convention honest
while making every cell prove delivery.

## Notes

Found while fixing 0448 (2026-08-06), by adding the sequence assertion to the
native matrix and watching the cpp/xrce cell fail against a constant that
encoded a different server's convention. The assertion was reverted and the
reason recorded at the call site rather than left as a passing-but-vacuous check.
