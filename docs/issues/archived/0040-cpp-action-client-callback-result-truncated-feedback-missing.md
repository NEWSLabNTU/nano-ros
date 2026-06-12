---
id: 40
title: C++ action-client callback path delivers a truncated result (`[0]`) and no feedback
status: resolved
type: bug
area: c-api
related: [phase-239, issue-0039]
---

## Resolution (Phase 239.14)

**Root cause: the action *server*, not the client callback path.** The C++
action-client callback code is correct. The truncated `[0]` result + zero
feedback were a downstream symptom of the **action server** running on a
degraded session: the stock `cpp_action_server` used
`nros::init_with_launch_auto`, which routes through the 3-arg `init` overload and
passes a **null locator** to the backend (issue #39). On that degraded session
the server mis-handled the goal request (observed: it parsed `order=1` — the
goal-id counter — instead of `order=10`, or failed to accept at all, flakily),
so it computed `[0]` / sent nothing.

Switching `examples/native/cpp/action-server` (and the callback client +
`service-client-callback`) to `nros::init()` (the env-var-fallback path used by
talker/listener) makes the action callback E2E deliver the **full** Fibonacci
result `[0,1,1,2,3,5,8,13,21,34]` + multiple feedback callbacks, reliably (3/3
runs). `test_cpp_action_communication_callback` now asserts the full sequence +
≥1 feedback.

Also corrected a genuine latent offset in `nros_cpp_action_client_poll`: the
result payload begins at offset **5** (`[CDR_HDR(4)][status(1)][payload]`), not 8
— the prior 8 would have skipped 3 bytes into a *valid* reply. Verified against
the C path (offset 5) which yields the full sequence.

The underlying null-locator init bug (**#39**) is also fixed: the 3-arg
`init` overload now applies the env fallback, so `init_with_launch_auto` works
everywhere. The callback examples use `init_with_launch_auto` like their siblings.

---

_Original report below._

The C++ action-client **callback** receive path
(`nros_cpp_action_client_poll` → `SendGoalOptions{goal_response, feedback,
result}`) dispatches all three callback kinds — but two of them carry wrong
payloads. Surfaced E2E by Phase 239.14
(`examples/native/cpp/action-client-callback` vs the stock `cpp_action_server`,
Fibonacci order=10):

```
Goal response (callback): ACCEPTED         <- correct
Result   (callback): status=4 sequence=[0] <- WRONG: should be [0,1,1,2,3,5,8,13,21,34,55]
Feedback callbacks: 0                       <- WRONG: server publishes feedback
```

Ground truth: the **C** action client (`nros_action_client_poll`, same
`ActionClientCore`) returns the full result
`[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]` for the identical server/goal.

## Two distinct defects

1. **Result truncated to `[0]`.** `status` (4 = SUCCEEDED) is read correctly,
   but the result message deserializes to a length-1 sequence `[0]`. This is
   *not* the payload offset: changing `RESULT_PAYLOAD_OFFSET` from 8 → 5 (to
   match the C path, which yields the full sequence) leaves the output at `[0]`,
   so the `result_buffer` content reaching `try_recv_get_result_reply` is itself
   short. Suspect the get-result reply matching / the shared `result_buffer`
   (also written by the goal-accept reply at `buf[4]`) in the C++ poll sequence
   — the async `get_result_async` + `poll()` ordering differs from the C
   blocking helper that works. (Phase 239 set the offset to 5 to align with C;
   the truncation is a separate, deeper bug.)

2. **Feedback never delivered (count 0).** The stock `cpp_action_server` runs
   the goal inline during accept and publishes feedback synchronously; the C++
   callback client polls feedback after the goal-response callback but receives
   none. Note the C poll path *does* fire feedback callbacks but with an **empty
   sequence** (`Feedback #N: []`) — its offset is `CDR_HEADER + UUID` (20)
   whereas the write framing is `CDR_HEADER + GoalId(4+16=20)` → payload at 24;
   so the C path under-reads too. Feedback framing should be derived from the
   `CdrReader` position after parsing the GoalId in `try_recv_feedback_raw`, not
   a magic constant, and unified across the C / C++ poll paths and
   `nros_action_try_recv_feedback`.

## Status of the surface

The **dispatch** mechanism (callbacks fire at `spin_once` via `poll()`) is
verified working by the Phase 239.14 E2E — goal-response acceptance and the
result callback (with correct SUCCEEDED status) both fire. Only the payload
extraction is wrong. The E2E asserts dispatch + acceptance + result-callback
firing; it deliberately does **not** assert the result sequence pending this
fix.

## Fix sketch

- Derive feedback/result payload offsets from the `CdrReader`/reply framing in
  `ActionClientCore`, exposing a single helper both C and C++ poll paths call —
  kill the per-callsite magic offsets (5/8 for result, 20/24 for feedback).
- Audit the C++ `get_result_async` → `poll()` reply matching so the full result
  message lands in `result_buffer` before `try_recv_get_result_reply` reads it.
- Then tighten the 239.14 E2E to assert the full Fibonacci sequence + ≥1
  feedback callback.
