---
id: 976
title: "Five action adapters in the Cyclone service path reshape the CDR to match ROS 2, and the only thing that exercises them is nano-ros talking to itself"
status: open
area: [rmw, testing]
severity: high
related: [0970, 0969, 0234, phase-171]
---

# The bytes they exist to correct are the one thing nothing checks

## What is in the service path

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp` carries five
per-action-type special cases in front of its generic typed path:

| adapter | what it does |
| --- | --- |
| `strip_goal_id_len_at` | removes a 4-byte `[16,0,0,0]` length prefix before the goal UUID |
| `strip_nested_cdr_at` | removes a nested `00 01 00 00` encapsulation header from INSIDE a message |
| `write_typed`'s `_SendGoal_*` / `_GetResult_Request_` branch | skips the CDR stream entirely and `memcpy`s the payload onto the typed struct |
| `write_fibonacci_get_result_response` | hand-builds the generated C struct, because `dds_stream_read_sample` crashes on that type (phase 171.0.b) |
| `take_fibonacci_get_result_response_wire` | the receive-side mirror of the above |

The first two are not "make cdrstream happy" adjustments. They **delete bytes**.
A `uint8[16]` UUID has no length prefix in CDR, and a CDR message has exactly one
encapsulation header — at the top, not before a nested member. So these correct a
serialization that would otherwise be wrong on the wire, and the correction lives
in the backend rather than at the source.

## What exercises them

Measured, by instrumenting every entry to `write_typed` and `take_typed_wire`
and running the backend's whole ctest suite:

```
7 PROBE take_typed_wire type=nros_test::srv::dds_::AddTwoInts_Response_
3 PROBE write_typed     type=nros_test::srv::dds_::AddTwoInts_Request_ len=40
3 PROBE write_typed     type=nros_test::srv::dds_::AddTwoInts_Response_ len=28
3 PROBE take_typed_wire type=nros_test::srv::dds_::AddTwoInts_Request_
```

`AddTwoInts` and nothing else. Not one action type reaches this file in 22 tests,
and both strip helpers fired **zero** times.

They are not dead code, though — `examples/fixtures.toml` carries a Cyclone
action pair (issue 0234), consumed by
`packages/testing/nros-tests/tests/native_api.rs::test_native_cyclonedds_rust_action`,
which drives an order-10 Fibonacci goal end to end. That lane runs the adapters.

**It is nano-ros server to nano-ros client.** Both ends share whatever convention
the adapters implement, so the test passes whether the bytes are ROS 2's or not.
The single property these five exist to provide — that an action's wire format
matches what a ROS 2 peer expects — is the one property no test can observe.

Compare the non-action paths, which do have an outside witness:
`ros2_pubsub_e2e` and `ros2_srv_e2e` run against stock `rmw_cyclonedds`. There is
no `ros2_action_e2e`.

## Why this surfaced now

**#0970** moved the message publisher
and subscriber onto a sertype whose sample is CDR, so neither direction decodes.
Its scope note left `service.cpp` alone, saying its request/reply path "reads
typed samples for several action types, and moving it belongs with the work that
retires those adapters".

That framing was incomplete. Two of the five do not merely read the typed
sample — they change what goes on the wire, and a blob sertype has nowhere to
change it. So migrating `service.cpp` would alter the action wire format, and by
the paragraph above **nothing in the tree could tell**.

So the migration is blocked, and not on effort. It is blocked on there being any
way to know whether it broke something.

## What unblocks it

A ROS 2 action interop test, in the shape the two existing ones already have:
`ros2_srv_e2e` spawns a real `ros2` process against an nros entity and compares.
The action equivalent — `ros2 action send_goal` against the nros action server,
and an nros action client against a `ros2` action server — turns the adapters'
whole purpose into something observable.

Then, and only then, one of two things becomes provable:

1. The corrections are right, ROS 2 interop passes with them, and migrating
   `service.cpp` requires moving each correction to the point where nano-ros
   SERIALIZES — which is where it belonged, since a `uint8[16]` written with a
   length prefix is wrong for every backend, not only this one. Note the message
   path already did exactly this: publisher.cpp's 233.6 comment records the Rust
   runtime being fixed to emit the fixed `octet[16]`, and the matching strip
   being deleted from both sides.
2. They are compensating for something that no longer exists, ROS 2 interop
   passes without them, and they are deleted.

Either way the answer comes from a test that has an outside witness, not from
reading the adapters.

## Not to be confused with

**#0970** (`0970-cyclone-rmw-should-own-its-sertype.md`, filed in PR #154 — not
linked because it has not landed on `main` yet) — the sertype migration
itself, whose message half has landed. This is the reason its service half has
not.
