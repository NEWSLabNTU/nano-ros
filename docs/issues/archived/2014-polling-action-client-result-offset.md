---
id: 2014
title: "C++ PollingActionClient decodes a get_result reply from byte 5; the payload starts at byte 8 — and action frame offsets were spelled at a dozen sites"
status: resolved
type: bug
severity: medium
area: [api]
found: 2026-10-24
resolved: 2026-10-24
related: [2011, 2009, 179]
---

## What

`rclcpp::PollingActionClient<A>::try_recv_result_sized` (`polling_action_client.hpp`,
`decode_result`) skipped a 5-byte prefix of the get_result reply and handed the
rest to `ResultType::ffi_deserialize`. The reply the server writes
(`action_core::write_get_result_header` + `reply_get_result_from_slab`) is

    [CDR encap 4][status i8][pad to 4 -> byte 8][result fields]

so the deserializer saw the three pad bytes and the first field byte as the
encapsulation header and decoded from byte 9. Every typed result on the polling
client failed to decode — or, for a type that does not check its length,
decoded garbage. The receive buffer was `Cap + 5` and truncated the last three
bytes of a full-size result as well.

`try_recv_feedback_sized` in the same header had the sibling defect: it passed
the whole raw feedback frame `[encap 4][GoalId 16][fields]` to the
deserializer, reading the goal id as the payload, into a buffer 16 bytes short
of the frame.

## Why it survived

The executor dispatch (`arena.rs`, `action_client_raw_try_process`) and the
C++ callback poll path (`nros_cpp_action_client_poll`) were fixed to byte 8 in
issue #179, but each consumer spelled the offsets itself — a `const` in each
function body, a `total_len >= 5 && buf[4]` acceptance test in five places, a
bare `GOAL_REQUEST_FRAMING_LEN` in two servers. The fix reached the sites the
symptom was seen at and not the header, which no test drove end to end.

The sweep also found two framing divergences of the same class: the C poll
path (`rclc_action_client_poll`'s feedback arm) handed the callback the bare
fields with NO encapsulation header while the executor dispatch handed the same
callback `[encap][fields]`, and the C result arm still sniffed for an encap the
executor dispatch retired (a body starting `00 01 00 00` reads as a header).
The C feedback / C++ feedback / goal trampolines prepended a hard-coded
little-endian header rather than the frame's own.

## Resolution

ONE spelling: `nros_node::action_frame` (`packages/core/nros-node/src/action_frame.rs`)
owns `RESULT_PAYLOAD_OFFSET` (8, derived from the status byte's CDR alignment),
`FEEDBACK_PAYLOAD_OFFSET` / `GOAL_REQUEST_PAYLOAD_OFFSET` (20),
`REPLY_STATUS_OFFSET` (4) and the readers over them — `goal_accepted`,
`result_status`, `result_fields`, `feedback_fields`, `feedback_goal_id`,
`goal_request_fields`, `cancel_return_code` and `splice` (the frame's own encap
in front of the fields). Every Rust consumer reads it: the raw and typed arena
dispatch, the C++ poll/try_recv/goal-trampoline/polling-server paths, and the C
poll/try_recv/goal-trampoline/polling-server paths.

The C/C++ headers that decode raw frames read the numbers through the C ABI:
`NROS_CPP_ACTION_{RESULT,FEEDBACK}_PAYLOAD_OFFSET` / `NROS_CPP_ACTION_CDR_HEADER_LEN`
and `RCLC_ACTION_{RESULT,FEEDBACK}_PAYLOAD_OFFSET`, cbindgen literals each bound
to `action_frame` by a `const` assert. `PollingActionClient` now decodes both
results and feedback through one `decode_spliced`, which moves the frame's
header in place to sit right before the fields — the same splice, no second
buffer — and sizes its buffers by the real prefix.

Test: `packages/api/nros-cpp/tests/compile/polling_action_result_offset_runtime.cpp`
(in `just check cpp`) round-trips results through `try_recv_result_sized` /
`try_recv_result` on the stub backend with non-zero pad bytes and a body that
looks like a header. Measured against the old header: 4 failures; with the fix:
OK. `action_frame`'s own unit tests pin the offsets.

Sweep:

    git grep -nE 'PAYLOAD_OFFSET|FRAMING_LEN|kPrefix|Cap \+ 5|\[4\] != 0|SEQ_PREFIX_LEN \+|4 \+ 16' -- 'packages/api/*' 'packages/core/*' ':!*/generated/*'

## Still open

`try_recv_feedback_sized` has no runtime test: the stub backend cannot stage a
subscription sample. Its decode shares `decode_spliced` with the result path.
