---
id: 448
title: "XRCE action client receives a 12-byte ZERO result reply, never the ROS 2 server's"
status: open
type: bug
area: rmw
related: [issue-0422, issue-0429, issue-0157]
---

## Symptom

`xrce_ros2_interop::test_ros2_action_xrce_client`, reproduced on three
consecutive retries:

```
ROS 2 DDS action server ↔ XRCE action client did not complete (233.6):
accepted=true got_feedback=false.
```

## What the flags imply

```rust
let accepted     = client_output.contains("Goal accepted");
let got_feedback = client_output.contains(&format!("{} [0, 1", ACTION_FEEDBACK_PREFIX));
```

with `ACTION_FEEDBACK_PREFIX = "Next number in sequence received:"`.

The output is collected by waiting for `ACTION_RESULT_PREFIX` ("Result
received:") with a 20 s timeout and `unwrap_or_default()`. On timeout that
yields an EMPTY string, which would make `accepted` false too — so
`accepted=true` proves the wait **succeeded** and the terminal result line
arrived.

So the client got its goal accepted AND its result, and only the feedback
assertion failed. That narrows it considerably: the send_goal request/reply and
the get_result path both crossed XRCE→agent→DDS successfully.

## Root observation (2026-08-06) — the reply is a 12-byte zero payload

A temporary dump of `result_buffer` in the client's result path
(`executor/arena.rs`, at `RESULT_PAYLOAD_OFFSET`) gives, on every run:

```
NROS_DEBUG result reply: total_len=12
  head=[00, 01, 00, 00,  00, 00, 00, 00,  00, 00, 00, 00]
         encap (XCDR1)     status = 0        seq_len = 0
```

Reading it off:

- `[0..4] = 00 01 00 00` — CDR_LE, i.e. **XCDR1**. So `begin_dheader()` is
  correctly a no-op here and the generated deserializer is NOT the problem
  (an early hypothesis; the dump refutes it).
- `[4] = 0` — `GoalStatus::Unknown`. A successful `rcl_action` result would be
  **4 (Succeeded)**.
- `[8..12] = 0` — sequence length zero, hence `Result received: []`.

`total_len` is **12**. The server's real reply for `order = 10` would be
4 (encap) + 1 (status) + 3 (pad) + 4 (length) + 11×4 (elements) = **56** bytes.

So the client is not mis-decoding the server's reply: it never receives it. It
receives a zeroed 12-byte stand-in.

## Why the server's own behaviour rules out a benign explanation

The test's ROS 2 server (`ros_env.rs`, `fibonacci_action_server`) is:

```python
order = goal_handle.request.order
seq = [0, 1]                       # <- before the loop
for i in range(1, order): ...
result.sequence = seq
```

`seq` is `[0, 1]` **before** the loop, so the server cannot return an empty
sequence for any `order`, not even 0. An empty result is therefore impossible to
produce legitimately — it can only come from the payload never arriving.

That also disposes of "the goal arrived with order=0": that would still yield
`[0, 1]`, plus the server prints `SERVER DONE {seq}` which the test could
capture to confirm whether `execute` ran at all.

## The `accepted` flag proves nothing

```rust
if ctx.send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal).is_ok() {
    state.sent = true;
    log::info!("Goal accepted by server, waiting for result");   // <- on SEND success
}
```

The line is emitted when the local **send** succeeds, before any server
response. The test's `accepted = client_output.contains("Goal accepted")` is
therefore satisfied by a goal that never reached the server. Any diagnosis that
starts from "acceptance works, so the services are fine" — including this
issue's first draft — is unfounded.

Worth fixing on its own: the message asserts a fact it has not observed. Left
here rather than changed blind, because several tests grep that exact string and
`action-client/src/lib.rs`'s own doc comment lists it as a test contract.

## Next step

Capture the ROS 2 server's stdout (`SERVER READY` / `SERVER DONE [...]`) in the
test and print it on failure. That splits the remaining space in one run:

- no `SERVER DONE` → the goal never reached the server; the send path is the
  subject.
- `SERVER DONE [0, 1, ...]` → the server ran and replied; the reply is being
  lost or replaced between the agent and the client.

## Notes

Found triaging #0422. The test retries 3× and fails identically each time, so it
is deterministic rather than a timing flake — which also argues against a
race in the feedback path and mildly toward (2).
