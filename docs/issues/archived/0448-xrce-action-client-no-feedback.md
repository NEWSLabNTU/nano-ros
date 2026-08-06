---
id: 448
title: The Rust action client shipped a SECOND CDR encapsulation in every goal request, so Fast-DDS dropped the sample and the client decoded a zeroed default result
status: resolved  # fixed 2026-08-06
type: bug
area: rmw
related: [issue-0422, issue-0429, issue-0453, rfc-0069, issue-0418]
---

## Symptom

`xrce_ros2_interop::test_ros2_action_xrce_client`, deterministic across three
retries:

```
ROS 2 DDS action server ↔ XRCE action client did not complete (233.6):
accepted=true got_feedback=false.
```

The client logged `Result received: []`, never any feedback. A dump of
`result_buffer` showed `total_len=12`, `head=[00,01,00,00, 00,00,00,00,
00,00,00,00]` — XCDR1 encap, `status=0` (Unknown, not 4=Succeeded), `seq_len=0`.
A real order-10 reply is 56 bytes. The test's ROS 2 server sets `seq = [0, 1]`
*before* its loop, so it cannot legitimately return an empty sequence — the
payload was never arriving, not mis-decoding.

## Root cause

`nros::send_goal` serialized the goal with `CdrWriter::new_with_header` and
passed the result to `send_goal_raw`, which frames the request itself. Every
goal therefore carried **two** encapsulation headers:

```
nano-ros: encap(4) + uuid(16) + encap(4) + order(4) = 28   <- 4 bytes over
ROS 2:    encap(4) + uuid(16) +            order(4) = 24
```

Capturing the ROS 2 server's stdout in the test is what produced the decisive
line — Fast-DDS sizes its reader history from the type and refused the sample
outright:

```
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '27' bytes and cannot be resized.
  -> Function can_change_be_added_nts
```

So the goal never reached the server: no `execute`, no feedback, and the client
decoded a zeroed default result.

Two early hypotheses were refuted by evidence and are recorded so they are not
re-run: the generated `begin_dheader()` wrap is a genuine no-op (`tx_writer`
emits XCDR1, confirmed by the `00 01 00 00` encap in the dump), and
`write_goal_id` correctly writes 16 raw bytes with no length prefix.

## Fix

`packages/api/nros/src/node.rs::send_goal` now uses the headerless
`CdrWriter::new`, matching its siblings `publish_feedback` and `complete_goal`,
which carry the identical RFC-0069 / issue 0418 rule in a comment. This was a
site 0418 missed: `nros-c` and `nros-cpp` already stripped the header
(`// C serialize produces [CDR_HEADER][fields] — strip the header.`) and the
C++ tick-ctx path is an unimplemented stub, so the Rust API was the only live
offender.

## Verification

`test_ros2_action_xrce_client` passes, with the server confirming receipt:

```
SERVER READY / SERVER GOAL order=10 / SERVER DONE [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

The test now asserts that sequence rather than the `Result received:` prefix
alone, wiring `output::FIBONACCI_ORDER_10_SEQUENCE` (which until now had **zero**
users). `native_example_reqresp_e2e` re-run: no regression.

## Why no native cell caught it

Issue 0453. The native action cells assert only the result PREFIX, and the Rust
example server ignores `goal.order` entirely — its output is byte-identical
whether the goal arrived or not. Every native action cell stayed green through
the whole bug.
