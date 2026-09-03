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

## The witness exists now, and the answer is (1) — measured 2026-09-03

`packages/testing/nros-tests/tests/ros2_action_e2e.rs`: a stock
`ros2 action send_goal`, over `rmw_cyclonedds_cpp`, against the nano-ros action
server (the `action-server` native Rust fixture built for Cyclone). Run against
ROS 2 Humble on this host:

```
Goal accepted with ID: 297f46db62f840c38ff15c0a58b862c3
Result:
    sequence: [0, 1, 1, 2, 3, 5]
Goal finished with status: SUCCEEDED
```

Discovery works too — `ros2 action info /fibonacci` reports `Action servers: 1`
and names the node.

**So the corrections are RIGHT.** A real ROS 2 client accepts the goal, receives
feedback, and gets the correct sequence back, which is option 1 above: ROS 2
interop passes WITH the adapters. They are not compensating for something that
no longer exists, and they must not simply be deleted.

The test asserts the RESULT CONTENT, not a zero exit: the adapters move bytes
inside the goal and result messages, so a wrong shape surfaces as a wrong
sequence or a goal never accepted. Checking only the exit status would pass on a
server that accepted the goal and computed nothing.

Mutation-tested rather than assumed — pointing it at an action nothing serves
fails on the goal-accepted assertion, so the witness can go red.

### What this unblocks, and what it does NOT settle

`service.cpp`'s migration (issue 0970's service half, and the third site of
issue 0969) now has something that would notice a wire-format change. That was
the whole blocker: "blocked on there being any way to know whether it broke
something".

Not settled: **where** the corrections belong. Option 1 above says a passing
interop test means migrating `service.cpp` requires moving each correction to
the point where nano-ros SERIALIZES — a `uint8[16]` written with a length prefix
is wrong for every backend, not only this one, and the message path already did
exactly that (publisher.cpp's 233.6 comment records the Rust runtime being fixed
to emit the fixed `octet[16]` and the matching strip being deleted from both
sides). This test makes that move verifiable; it does not perform it.

Also not covered: the reverse direction, an nros action CLIENT against a `ros2`
action server. The adapters sit on both sides of the service path, and this
witnesses one. That is the obvious next cell.

## MEASURED with the witness: the two write-side strips are DEAD (2026-09-03)

The design call — if a correction is about ROS 2 compatibility and correctness,
nano-ros should own it, not one backend — turns out to be already SATISFIED on
the write path, and the backend code is vestigial.

Instrumented both branches of `write_typed` and rebuilt the native fixtures
(`strings` confirms 4 probe literals in each binary, so the instrumentation is
linked rather than assumed). Then ran the nano-ros action client against the
stock ROS 2 server:

```
2  PROBE strip_goal_id_len_at declined
1  PROBE strip_nested_cdr_at(SendGoal) declined
1  [INFO] Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

They are REACHED and they DECLINE — every time — while the round trip succeeds
against a real ROS 2 peer. Their guards test for the `[16,0,0,0]` length prefix
and a nested encapsulation header, and neither is present: the Rust runtime
already emits the fixed `octet[16]` (publisher.cpp's 233.6 note records that
fix), so there is nothing left to strip.

**So nano-ros already owns this correction.** The bytes are right at the source,
which is where a ROS 2-compatibility fix belongs, and
`strip_goal_id_len_at` / `strip_nested_cdr_at` are dead code on the write path.

### One measurement error worth recording

The first run of this reported "no probe fired at all", which would have meant
`write_typed` was off the action path entirely. Wrong, and wrong in the
convenient direction: the reverse cell CAPTURES the client's output into the
assertion string, so the probe lines went into the capture rather than the
terminal. Absence of evidence was an artifact of where the evidence was sent.
Running the client directly is what produced the table above.

### What this licenses, and what it does not

Licensed: deleting the two write-side strips, and the `type_ends_with` branches
that call them.

NOT licensed yet: the same claim for the C and C++ action paths. This measured
the RUST runtime. `nros-c`/`nros-cpp` action entries reach `write_typed` through
the same C ABI but serialize through their own generated code, and nothing here
observed them. The same probe against a C/C++ action entry is the remaining
check, and it is cheap now that the witness exists.

Also untouched: the three RECEIVE-side adapters
(`take_fibonacci_get_result_response_wire` and the two `_SendGoal_*` /
`_GetResult_Request_` memcpy branches). Those are a different question — issue
0969 argues a `dds_takecdr` rewrite removes the need for them rather than
correcting them.

## Not to be confused with

**#0970** (`0970-cyclone-rmw-should-own-its-sertype.md`, filed in PR #154 — not
linked because it has not landed on `main` yet) — the sertype migration
itself, whose message half has landed. This is the reason its service half has
not.
